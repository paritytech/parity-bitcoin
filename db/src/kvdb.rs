//! Key-Value store abstraction with `RocksDB` backend.

use std::{self, fs, mem};
use std::io::ErrorKind;
use std::collections::HashMap;
use std::path::PathBuf;
use rocksdb::{DB, Writable, WriteBatch, WriteOptions, IteratorMode, DBIterator,
	Options, DBCompactionStyle, BlockBasedOptions, Cache, Column};
use elastic_array::ElasticArray32;
use parking_lot::RwLock;
use primitives::bytes::Bytes;
use byteorder::{LittleEndian, ByteOrder};

/// Database error
pub enum Error {
	/// Rocksdb error
	DB(String),
	/// Io error
	Io(std::io::Error),
}

impl From<String> for Error {
	fn from(err: String) -> Error {
		Error::DB(err)
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Error {
		Error::Io(err)
	}
}

const DB_BACKGROUND_FLUSHES: i32 = 2;
const DB_BACKGROUND_COMPACTIONS: i32 = 2;

/// Write transaction. Batches a sequence of put/delete operations for efficiency.
pub struct DBTransaction {
	ops: Vec<DBOp>,
	rollback_len: Option<usize>,
}

enum DBOp {
	Insert {
		col: Option<u32>,
		key: ElasticArray32<u8>,
		value: Bytes,
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
			value: value.to_vec().into(),
		});
	}

	/// Insert a key-value pair in the transaction. Any existing value value will be overwritten upon write.
	pub fn put_vec(&mut self, col: Option<u32>, key: &[u8], value: Bytes) {
		let mut ekey = ElasticArray32::new();
		ekey.append_slice(key);
		self.ops.push(DBOp::Insert {
			col: col,
			key: ekey,
			value: value,
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

	/// Write u64
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
	Insert(Bytes),
	Delete,
}

/// Compaction profile for the database settings
#[derive(Clone, Copy)]
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
		CompactionProfile {
			initial_file_size: 32 * 1024 * 1024,
			file_size_multiplier: 2,
			write_rate_limit: None,
		}
	}
}

impl CompactionProfile {
	/// Slow hdd compaction profile
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
	config: DatabaseConfig,
	write_opts: WriteOptions,
	overlay: RwLock<Vec<HashMap<ElasticArray32<u8>, KeyState>>>,
	path: String,
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

		let mut cfs: Vec<Column> = Vec::new();
		let db = match config.columns {
			Some(columns) => {
				let cfnames: Vec<_> = (0..columns).map(|c| format!("col{}", c)).collect();
				let cfnames: Vec<&str> = cfnames.iter().map(|n| n as &str).collect();
				match DB::open_cf(&opts, path, &cfnames, &cf_options) {
					Ok(db) => {
						cfs = cfnames.iter().map(|n| db.cf_handle(n).unwrap()).collect();
						assert!(cfs.len() == columns as usize);
						Ok(db)
					}
					Err(_) => {
						// retry and create CFs
						match DB::open_cf(&opts, path, &[], &[]) {
							Ok(mut db) => {
								cfs = cfnames.iter().enumerate().map(|(i, n)| db.create_cf(n, &cf_options[i]).unwrap()).collect();
								Ok(db)
							},
							err => err,
						}
					}
				}
			},
			None => DB::open(&opts, path)
		};
		let db = match db {
			Ok(db) => db,
			Err(ref s) if s.starts_with("Corruption:") => {
				try!(DB::repair(&opts, path));
				try!(DB::open(&opts, path))
			},
			Err(s) => { return Err(s); }
		};
		let num_cols = cfs.len();
		Ok(Database {
			db: RwLock::new(Some(DBAndColumns{ db: db, cfs: cfs })),
			config: config.clone(),
			write_opts: write_opts,
			overlay: RwLock::new((0..(num_cols + 1)).map(|_| HashMap::new()).collect()),
			path: path.to_owned(),
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

	/// Commit buffered changes to database.
	pub fn flush(&self) -> Result<(), String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let batch = WriteBatch::new();
				let mut overlay = self.overlay.write();

				for (c, column) in overlay.iter_mut().enumerate() {
					let column_data = mem::replace(column, HashMap::new());
					for (key, state) in column_data {
						match state {
							KeyState::Delete => {
								if c > 0 {
									try!(batch.delete_cf(cfs[c - 1], &key));
								} else {
									try!(batch.delete(&key));
								}
							},
							KeyState::Insert(value) => {
								if c > 0 {
									try!(batch.put_cf(cfs[c - 1], &key, &value));
								} else {
									try!(batch.put(&key, &value));
								}
							},
						}
					}
				}
				db.write_opt(batch, &self.write_opts)
			},
			None => Err("Database is closed".to_owned())
		}
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
	pub fn get(&self, col: Option<u32>, key: &[u8]) -> Result<Option<Bytes>, String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let overlay = &self.overlay.read()[Self::to_overlay_column(col)];
				match overlay.get(key) {
					Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
					Some(&KeyState::Delete) => Ok(None),
					None => {
						col.map_or_else(
							|| db.get(key).map(|r| r.map(|v| v.to_vec().into())),
							|c| db.get_cf(cfs[c as usize], key).map(|r| r.map(|v| v.to_vec().into())))
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
				col.map_or_else(|| DatabaseIterator { iter: db.iterator(IteratorMode::Start) },
					|c| DatabaseIterator { iter: db.iterator_cf(cfs[c as usize], IteratorMode::Start).unwrap() })
			},
			None => panic!("Not supported yet") //TODO: return an empty iterator or change return type
		}
	}

	/// Close the database
	fn close(&self) {
		*self.db.write() = None;
		self.overlay.write().clear();
	}

	/// Restore the database from a copy at given path.
	pub fn restore(&self, new_db: &str) -> Result<(), Error> {
		self.close();

		let mut backup_db = PathBuf::from(&self.path);
		backup_db.pop();
		backup_db.push("backup_db");

		let existed = match fs::rename(&self.path, &backup_db) {
			Ok(_) => true,
			Err(e) => if let ErrorKind::NotFound = e.kind() {
				false
			} else {
				return Err(e.into());
			}
		};

		match fs::rename(&new_db, &self.path) {
			Ok(_) => {
				// clean up the backup.
				if existed {
					try!(fs::remove_dir_all(&backup_db));
				}
			}
			Err(e) => {
				// restore the backup.
				if existed {
					try!(fs::rename(&backup_db, &self.path));
				}
				return Err(e.into())
			}
		}

		// reopen the database and steal handles into self
		let db = try!(Self::open(&self.config, &self.path));
		*self.db.write() = mem::replace(&mut *db.db.write(), None);
		*self.overlay.write() = mem::replace(&mut *db.overlay.write(), Vec::new());
		Ok(())
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
