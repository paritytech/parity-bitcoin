//! Key-Value store abstraction with `RocksDB` backend.

use std::mem;
use parking_lot::{Mutex, MutexGuard, RwLock};
use elastic_array::*;
use std::default::Default;
use rocksdb::{DB, Writable, WriteBatch, WriteOptions, IteratorMode, DBIterator,
	Options, DBCompactionStyle, BlockBasedOptions, Cache, Column, ReadOptions};
#[cfg(target_os = "linux")]
use regex::Regex;
#[cfg(target_os = "linux")]
use std::process::Command;
#[cfg(target_os = "linux")]
use std::fs::File;
use std::collections::HashMap;
use byteorder::{LittleEndian, ByteOrder};
//use std::path::Path;

const DB_BACKGROUND_FLUSHES: i32 = 2;
const DB_BACKGROUND_COMPACTIONS: i32 = 2;

type Bytes = Vec<u8>;

pub type DBValue = ElasticArray128<u8>;

/// Write transaction. Batches a sequence of put/delete operations for efficiency.
pub struct DBTransaction {
	ops: Vec<DBOp>,
	rollback_len: Option<usize>,
}

enum DBOp {
	Insert {
		col: Option<u32>,
		key: ElasticArray32<u8>,
		value: DBValue,
	},
	Delete {
		col: Option<u32>,
		key: ElasticArray32<u8>,
	}
}

impl DBTransaction {
	/// Create new transaction.
	pub fn new(_db: &Database) -> DBTransaction {
		DBTransaction {
			ops: Vec::with_capacity(256),
			rollback_len: None,
		}
	}

	/// Insert a key-value pair in the transaction. Any existing value value will be overwritten upon write.
	pub fn put(&mut self, col: Option<u32>, key: &[u8], value: &[u8]) {
		let mut ekey = ElasticArray32::new();
		ekey.append_slice(key);
		self.ops.push(DBOp::Insert {
			col: col,
			key: ekey,
			value: DBValue::from_slice(value),
		});
	}

	/// Insert a key-value pair in the transaction. Any existing value value will be overwritten upon write.
	pub fn put_vec(&mut self, col: Option<u32>, key: &[u8], value: Bytes) {
		let mut ekey = ElasticArray32::new();
		ekey.append_slice(key);
		self.ops.push(DBOp::Insert {
			col: col,
			key: ekey,
			value: DBValue::from_vec(value),
		});
	}

	/// Delete value by key.
	pub fn delete(&mut self, col: Option<u32>, key: &[u8]) {
		let mut ekey = ElasticArray32::new();
		ekey.append_slice(key);
		self.ops.push(DBOp::Delete {
			col: col,
			key: ekey,
		});
	}

	pub fn write_u64(&mut self, col: Option<u32>, key: &[u8], value: u64) {
		let mut val = [0u8; 8];
		LittleEndian::write_u64(&mut val, value);
		self.put(col, key, &val);
	}

	/// Write u32
	pub fn write_u32(&mut self, col: Option<u32>, key: &[u8], value: u32) {
		let mut val = [0u8; 4];
		LittleEndian::write_u32(&mut val, value);
		self.put(col, key, &val);
	}

	pub fn remember(&mut self) {
		self.rollback_len = Some(self.ops.len());
	}

	pub fn rollback(&mut self) {
		if let Some(len) = self.rollback_len {
			self.ops.truncate(len);
		}
	}
}

enum KeyState {
	Insert(DBValue),
	Delete,
}

/// Compaction profile for the database settings
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CompactionProfile {
	/// L0-L1 target file size
	pub initial_file_size: u64,
	/// L2-LN target file size multiplier
	pub file_size_multiplier: i32,
	/// rate limiter for background flushes and compactions, bytes/sec, if any
	pub write_rate_limit: Option<u64>,
}

impl Default for CompactionProfile {
	/// Default profile suitable for most storage
	fn default() -> CompactionProfile {
		CompactionProfile::ssd()
	}
}

/// Given output of df command return Linux rotational flag file path.
#[cfg(target_os = "linux")]
pub fn rotational_from_df_output(df_out: Vec<u8>) -> Option<PathBuf> {
	str::from_utf8(df_out.as_slice())
		.ok()
		// Get the drive name.
		.and_then(|df_str| Regex::new(r"/dev/(sd[:alpha:]{1,2})")
			.ok()
			.and_then(|re| re.captures(df_str))
			.and_then(|captures| captures.at(1)))
		// Generate path e.g. /sys/block/sda/queue/rotational
		.map(|drive_path| {
			let mut p = PathBuf::from("/sys/block");
			p.push(drive_path);
			p.push("queue/rotational");
			p
		})
}

impl CompactionProfile {

	/// Default profile suitable for SSD storage
	pub fn ssd() -> CompactionProfile {
		CompactionProfile {
			initial_file_size: 32 * 1024 * 1024,
			file_size_multiplier: 2,
			write_rate_limit: None,
		}
	}

	/// Slow HDD compaction profile
	pub fn hdd() -> CompactionProfile {
		CompactionProfile {
			initial_file_size: 192 * 1024 * 1024,
			file_size_multiplier: 1,
			write_rate_limit: Some(8 * 1024 * 1024),
		}
	}
}

/// Database configuration
#[derive(Clone)]
pub struct DatabaseConfig {
	/// Max number of open files.
	pub max_open_files: i32,
	/// Cache sizes (in MiB) for specific columns.
	pub cache_sizes: HashMap<Option<u32>, usize>,
	/// Compaction profile
	pub compaction: CompactionProfile,
	/// Set number of columns
	pub columns: Option<u32>,
	/// Should we keep WAL enabled?
	pub wal: bool,
}

impl DatabaseConfig {
	/// Create new `DatabaseConfig` with default parameters and specified set of columns.
	/// Note that cache sizes must be explicitly set.
	pub fn with_columns(columns: Option<u32>) -> Self {
		let mut config = Self::default();
		config.columns = columns;
		config
	}

	/// Set the column cache size in MiB.
	pub fn set_cache(&mut self, col: Option<u32>, size: usize) {
		self.cache_sizes.insert(col, size);
	}
}

impl Default for DatabaseConfig {
	fn default() -> DatabaseConfig {
		DatabaseConfig {
			cache_sizes: HashMap::new(),
			max_open_files: 512,
			compaction: CompactionProfile::default(),
			columns: None,
			wal: true,
		}
	}
}

/// Database iterator for flushed data only
pub struct DatabaseIterator {
	iter: DBIterator,
}

impl<'a> Iterator for DatabaseIterator {
	type Item = (Box<[u8]>, Box<[u8]>);

    fn next(&mut self) -> Option<Self::Item> {
		self.iter.next()
	}
}

struct DBAndColumns {
	db: DB,
	cfs: Vec<Column>,
}

/// Key-Value database.
pub struct Database {
	db: RwLock<Option<DBAndColumns>>,
	write_opts: WriteOptions,
	read_opts: ReadOptions,
	// Dirty values added with `write_buffered`. Cleaned on `flush`.
	overlay: RwLock<Vec<HashMap<ElasticArray32<u8>, KeyState>>>,
	// Values currently being flushed. Cleared when `flush` completes.
	flushing: RwLock<Vec<HashMap<ElasticArray32<u8>, KeyState>>>,
	// Prevents concurrent flushes.
	// Value indicates if a flush is in progress.
	flushing_lock: Mutex<bool>,
}

impl Database {
	/// Open database with default settings.
	pub fn open_default(path: &str) -> Result<Database, String> {
		Database::open(&DatabaseConfig::default(), path)
	}

	/// Open database file. Creates if it does not exist.
	pub fn open(config: &DatabaseConfig, path: &str) -> Result<Database, String> {
		// default cache size for columns not specified.
		const DEFAULT_CACHE: usize = 2;

		let mut opts = Options::new();
		if let Some(rate_limit) = config.compaction.write_rate_limit {
			try!(opts.set_parsed_options(&format!("rate_limiter_bytes_per_sec={}", rate_limit)));
		}
		try!(opts.set_parsed_options(&format!("max_total_wal_size={}", 64 * 1024 * 1024)));
		try!(opts.set_parsed_options("verify_checksums_in_compaction=0"));
		opts.set_max_open_files(config.max_open_files);
		opts.create_if_missing(true);
		opts.set_use_fsync(false);

		opts.set_max_background_flushes(DB_BACKGROUND_FLUSHES);
		opts.set_max_background_compactions(DB_BACKGROUND_COMPACTIONS);

		// compaction settings
		opts.set_compaction_style(DBCompactionStyle::DBUniversalCompaction);
		opts.set_target_file_size_base(config.compaction.initial_file_size);
		opts.set_target_file_size_multiplier(config.compaction.file_size_multiplier);

		let mut cf_options = Vec::with_capacity(config.columns.unwrap_or(0) as usize);
		let cfnames: Vec<_> = (0..config.columns.unwrap_or(0)).map(|c| format!("col{}", c)).collect();
		let cfnames: Vec<&str> = cfnames.iter().map(|n| n as &str).collect();

		for col in 0 .. config.columns.unwrap_or(0) {
			let mut opts = Options::new();
			opts.set_compaction_style(DBCompactionStyle::DBUniversalCompaction);
			opts.set_target_file_size_base(config.compaction.initial_file_size);
			opts.set_target_file_size_multiplier(config.compaction.file_size_multiplier);

			let col_opt = config.columns.map(|_| col);

			{
				let cache_size = config.cache_sizes.get(&col_opt).cloned().unwrap_or(DEFAULT_CACHE);
				let mut block_opts = BlockBasedOptions::new();
				// all goes to read cache.
				block_opts.set_cache(Cache::new(cache_size * 1024 * 1024));
				opts.set_block_based_table_factory(&block_opts);
			}

			cf_options.push(opts);
		}

		let mut write_opts = WriteOptions::new();
		if !config.wal {
			write_opts.disable_wal(true);
		}
		let mut read_opts = ReadOptions::new();
		read_opts.set_verify_checksums(false);

		let mut cfs: Vec<Column> = Vec::new();
		let db = match config.columns {
			Some(columns) => {
				match DB::open_cf(&opts, path, &cfnames, &cf_options) {
					Ok(db) => {
						cfs = cfnames.iter().map(|n| db.cf_handle(n)
							.expect("rocksdb opens a cf_handle for each cfname; qed")).collect();
						assert!(cfs.len() == columns as usize);
						Ok(db)
					}
					Err(_) => {
						// retry and create CFs
						match DB::open_cf(&opts, path, &[], &[]) {
							Ok(mut db) => {
								cfs = try!(cfnames.iter().enumerate().map(|(i, n)| db.create_cf(n, &cf_options[i])).collect());
								Ok(db)
							},
							err @ Err(_) => err,
						}
					}
				}
			},
			None => DB::open(&opts, path)
		};

		let db = match db {
			Ok(db) => db,
			Err(ref s) if s.starts_with("Corruption:") => {
				info!("{}", s);
				info!("Attempting DB repair for {}", path);
				try!(DB::repair(&opts, path));

				match cfnames.is_empty() {
					true => try!(DB::open(&opts, path)),
					false => try!(DB::open_cf(&opts, path, &cfnames, &cf_options))
				}
			},
			Err(s) => { return Err(s); }
		};
		let num_cols = cfs.len();
		Ok(Database {
			db: RwLock::new(Some(DBAndColumns{ db: db, cfs: cfs })),
			write_opts: write_opts,
			overlay: RwLock::new((0..(num_cols + 1)).map(|_| HashMap::new()).collect()),
			flushing: RwLock::new((0..(num_cols + 1)).map(|_| HashMap::new()).collect()),
			flushing_lock: Mutex::new((false)),
			read_opts: read_opts,
		})
	}

	/// Creates new transaction for this database.
	pub fn transaction(&self) -> DBTransaction {
		DBTransaction::new(self)
	}


	fn to_overlay_column(col: Option<u32>) -> usize {
		col.map_or(0, |c| (c + 1) as usize)
	}

	/// Commit transaction to database.
	pub fn write_buffered(&self, tr: DBTransaction) {
		let mut overlay = self.overlay.write();
		let ops = tr.ops;
		for op in ops {
			match op {
				DBOp::Insert { col, key, value } => {
					let c = Self::to_overlay_column(col);
					overlay[c].insert(key, KeyState::Insert(value));
				},
				DBOp::Delete { col, key } => {
					let c = Self::to_overlay_column(col);
					overlay[c].insert(key, KeyState::Delete);
				},
			}
		};
	}

	/// Commit buffered changes to database. Must be called under `flush_lock`
	fn write_flushing_with_lock(&self, _lock: &mut MutexGuard<bool>) -> Result<(), String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let batch = WriteBatch::new();
				mem::swap(&mut *self.overlay.write(), &mut *self.flushing.write());
				{
					for (c, column) in self.flushing.read().iter().enumerate() {
						for (ref key, ref state) in column.iter() {
							match **state {
								KeyState::Delete => {
									if c > 0 {
										try!(batch.delete_cf(cfs[c - 1], &key));
									} else {
										try!(batch.delete(&key));
									}
								},
								KeyState::Insert(ref value) => {
									if c > 0 {
										try!(batch.put_cf(cfs[c - 1], &key, value));
									} else {
										try!(batch.put(&key, &value));
									}
								},
							}
						}
					}
				}
				try!(db.write_opt(batch, &self.write_opts));
				for column in self.flushing.write().iter_mut() {
					column.clear();
					column.shrink_to_fit();
				}
				Ok(())
			},
			None => Err("Database is closed".to_owned())
		}
	}

	/// Commit buffered changes to database.
	pub fn flush(&self) -> Result<(), String> {
		let mut lock = self.flushing_lock.lock();
		// If RocksDB batch allocation fails the thread gets terminated and the lock is released.
		// The value inside the lock is used to detect that.
		if *lock {
			// This can only happen if another flushing thread is terminated unexpectedly.
			return Err("Database write failure. Running low on memory perhaps?".to_owned());
		}
		*lock = true;
		let result = self.write_flushing_with_lock(&mut lock);
		*lock = false;
		result
	}

	/// Commit transaction to database.
	pub fn write(&self, tr: DBTransaction) -> Result<(), String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let batch = WriteBatch::new();
				let ops = tr.ops;
				for op in ops {
					match op {
						DBOp::Insert { col, key, value } => {
							try!(col.map_or_else(|| batch.put(&key, &value), |c| batch.put_cf(cfs[c as usize], &key, &value)))
						},
						DBOp::Delete { col, key } => {
							try!(col.map_or_else(|| batch.delete(&key), |c| batch.delete_cf(cfs[c as usize], &key)))
						},
					}
				}
				db.write_opt(batch, &self.write_opts)
			},
			None => Err("Database is closed".to_owned())
		}
	}

	/// Get value by key.
	pub fn get(&self, col: Option<u32>, key: &[u8]) -> Result<Option<DBValue>, String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let overlay = &self.overlay.read()[Self::to_overlay_column(col)];
				match overlay.get(key) {
					Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
					Some(&KeyState::Delete) => Ok(None),
					None => {
						let flushing = &self.flushing.read()[Self::to_overlay_column(col)];
						match flushing.get(key) {
							Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
							Some(&KeyState::Delete) => Ok(None),
							None => {
								col.map_or_else(
									|| db.get_opt(key, &self.read_opts).map(|r| r.map(|v| DBValue::from_slice(&v))),
									|c| db.get_cf_opt(cfs[c as usize], key, &self.read_opts).map(|r| r.map(|v| DBValue::from_slice(&v))))
							},
						}
					},
				}
			},
			None => Ok(None),
		}
	}

	/// Get database iterator for flushed data.
	pub fn iter(&self, col: Option<u32>) -> DatabaseIterator {
		//TODO: iterate over overlay
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				col.map_or_else(|| DatabaseIterator { iter: db.iterator_opt(IteratorMode::Start, &self.read_opts) },
					|c| DatabaseIterator { iter: db.iterator_cf_opt(cfs[c as usize], IteratorMode::Start, &self.read_opts)
						.expect("iterator params are valid; qed") })
			},
			None => panic!("Not supported yet") //TODO: return an empty iterator or change return type
		}
	}

	/// Close the database
	pub fn close(&self) {
		*self.db.write() = None;
		self.overlay.write().clear();
		self.flushing.write().clear();
	}
}

impl Drop for Database {
	fn drop(&mut self) {
		// write all buffered changes if we can.
		let _ = self.flush();
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use devtools::*;

	fn test_db(config: &DatabaseConfig) {
		let path = RandomTempPath::create_dir();
		let db = Database::open(config, path.as_path().to_str().unwrap()).unwrap();

		let key1 = b"key1";
		let key2 = b"key2";
		let key3 = b"key3";

		let mut batch = db.transaction();
		batch.put(None, key1, b"cat");
		batch.put(None, key2, b"dog");
		db.write(batch).unwrap();

		assert_eq!(&*db.get(None, key1).unwrap().unwrap(), b"cat");

		let contents: Vec<_> = db.iter(None).collect();
		assert_eq!(contents.len(), 2);
		assert_eq!(&*contents[0].0, &*key1);
		assert_eq!(&*contents[0].1, b"cat");
		assert_eq!(&*contents[1].0, &*key2);
		assert_eq!(&*contents[1].1, b"dog");

		let mut batch = db.transaction();
		batch.delete(None, key1);
		db.write(batch).unwrap();

		assert!(db.get(None, key1).unwrap().is_none());

		let mut batch = db.transaction();
		batch.put(None, key1, b"cat");
		db.write(batch).unwrap();

		let mut transaction = db.transaction();
		transaction.put(None, key3, b"elephant");
		transaction.delete(None, key1);
		db.write(transaction).unwrap();
		assert_eq!(&*db.get(None, key3).unwrap().unwrap(), b"elephant");
	}

	#[test]
	fn kvdb() {
		let path = RandomTempPath::create_dir();
		let _ = Database::open_default(path.as_path().to_str().unwrap()).unwrap();
		test_db(&DatabaseConfig::default());
	}
}
