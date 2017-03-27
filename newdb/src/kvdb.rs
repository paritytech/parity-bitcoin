//! Key-Value store abstraction with `RocksDB` backend.

use std::mem;
use std::collections::HashMap;
use parking_lot::{Mutex, MutexGuard, RwLock};
use elastic_array::{ElasticArray32, ElasticArray128};
use rocksdb::{
	DB, Writable, WriteBatch, WriteOptions, IteratorMode, DBIterator,
	Options, DBCompactionStyle, BlockBasedOptions, Cache, Column, ReadOptions
};
use byteorder::{LittleEndian, ByteOrder};
use transaction::{Transaction, Operation, Location, Key, Value};

const DB_BACKGROUND_FLUSHES: i32 = 2;
const DB_BACKGROUND_COMPACTIONS: i32 = 2;

enum KeyState {
	Insert(Value),
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
	/// Specific number of bloom filter bits, if any
	pub bloom_filters: HashMap<Option<u32>, u8>,
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
			bloom_filters: HashMap::new(),
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
	/// Dirty values added with `write_buffered`. Cleaned on `flush`.
	overlay: RwLock<HashMap<Location, HashMap<Key, KeyState>>>,
	/// Values currently being flushed. Cleared when `flush` completes.
	flushing: RwLock<HashMap<Location, HashMap<Key, KeyState>>>,
	/// Prevents concurrent flushes.
	/// Value indicates if a flush is in progress.
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
				let mut block_opts = BlockBasedOptions::new();
				let cache_size = config.cache_sizes.get(&col_opt).cloned().unwrap_or(DEFAULT_CACHE);
				if let Some(bloom_bits) = config.bloom_filters.get(&col_opt) {
					block_opts.set_filter(bloom_bits.clone() as i32);
				}
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
			overlay: RwLock::default(),
			flushing: RwLock::default(),
			flushing_lock: Mutex::new((false)),
			read_opts: read_opts,
		})
	}

	/// Commit transaction to database.
	pub fn write_buffered(&self, tx: Transaction) {
		let mut overlay = self.overlay.write();
		for op in tx.operations.into_iter() {
			match op {
				Operation::Insert { location, key, value } => {
					let overlay = overlay.entry(location).or_insert_with(HashMap::default);
					overlay.insert(key, KeyState::Insert(value));
				},
				Operation::Delete { location, key } => {
					let overlay = overlay.entry(location).or_insert_with(HashMap::default);
					overlay.insert(key, KeyState::Delete);
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
					for (location, keypairs) in self.flushing.read().iter() {
						for (ref key, ref state) in keypairs {
							match **state {
								KeyState::Delete => match *location {
									Location::DB => {
										batch.delete(key)?;
									},
									Location::Column(column) => {
										batch.delete_cf(cfs[column as usize], key)?;
									},
								},
								KeyState::Insert(ref value) => match *location {
									Location::DB => {
										batch.put(key, value)?;
									},
									Location::Column(column) => {
										batch.put_cf(cfs[column as usize], key, value)?;
									}
								},
							}
						}
					}
				}
				try!(db.write_opt(batch, &self.write_opts));
				let mut flushing = self.flushing.write();
				flushing.clear();
				flushing.shrink_to_fit();
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
	pub fn write(&self, tx: Transaction) -> Result<(), String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				let batch = WriteBatch::new();
				for op in tx.operations.into_iter() {
					match op {
						Operation::Insert { location, key, value } => {
							match location {
								Location::DB => {
									batch.put(&key, &value)?;
								},
								Location::Column(col) => {
									batch.put_cf(cfs[col as usize], &key, &value)?;
								}
							}
						},
						Operation::Delete { location, key } => {
							match location {
								Location::DB => {
									batch.delete(&key)?;
								},
								Location::Column(col) => {
									batch.delete_cf(cfs[col as usize], &key)?;
								}
							}
						},
					}
				}
				db.write_opt(batch, &self.write_opts)
			},
			None => Err("Database is closed".to_owned())
		}
	}

	/// Get value by key.
	pub fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String> {
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				match self.overlay.read().get(&location).and_then(|overlay| overlay.get(key)) {
					Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
					Some(&KeyState::Delete) => Ok(None),
					None => {
						match self.flushing.read().get(&location).and_then(|flushing| flushing.get(key)) {
							Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
							Some(&KeyState::Delete) => Ok(None),
							None => {
								match location {
									Location::DB => {
										let value = db.get_opt(key, &self.read_opts)?;
										Ok(value.map(|v| Value::from_slice(&v)))
									},
									Location::Column(col) => {
										let value = db.get_cf_opt(cfs[col as usize], key, &self.read_opts)?;
										Ok(value.map(|v| Value::from_slice(&v)))
									}
								}
							},
						}
					},
				}
			},
			None => Ok(None),
		}
	}

	/// Close the database
	pub fn close(&self) {
		*self.db.write() = None;
		self.overlay.write().clear();
		self.flushing.write().clear();
	}

	pub fn iter(&self, location: Location) -> DatabaseIterator {
		//TODO: iterate over overlay
		match *self.db.read() {
			Some(DBAndColumns { ref db, ref cfs }) => {
				match location {
					Location::DB => {
						DatabaseIterator {
							iter: db.iterator_opt(IteratorMode::Start, &self.read_opts)
						}
					},
					Location::Column(column) => {
						DatabaseIterator {
							iter: db.iterator_cf_opt(cfs[column as usize], IteratorMode::Start, &self.read_opts)
								.expect("iterator params are valid; qed")
						}
					}
				}
			},
			None => panic!("Not supported yet") //TODO: return an empty iterator or change return type
		}
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
	extern crate ethcore_devtools as devtools;

	use transaction::{Transaction, Location};
	use super::*;
	use self::devtools::*;

	fn test_db(config: &DatabaseConfig) {
		let path = RandomTempPath::create_dir();
		let db = Database::open(config, path.as_path().to_str().unwrap()).unwrap();

		let key1 = b"key1";
		let key2 = b"key2";
		let key3 = b"key3";

		let mut batch = Transaction::default();
		batch.insert(Location::DB, key1, b"cat");
		batch.insert(Location::DB, key2, b"dog");
		db.write(batch).unwrap();

		assert_eq!(&*db.get(Location::DB, key1).unwrap().unwrap(), b"cat");

		let contents: Vec<_> = db.iter(Location::DB).collect();
		assert_eq!(contents.len(), 2);
		assert_eq!(&*contents[0].0, &*key1);
		assert_eq!(&*contents[0].1, b"cat");
		assert_eq!(&*contents[1].0, &*key2);
		assert_eq!(&*contents[1].1, b"dog");

		let mut batch = Transaction::default();
		batch.delete(Location::DB, key1);
		db.write(batch).unwrap();

		assert_eq!(db.get(Location::DB, key1).unwrap(), None);

		let mut batch = Transaction::default();
		batch.insert(Location::DB, key1, b"cat");
		db.write(batch).unwrap();

		let mut transaction = Transaction::default();
		transaction.insert(Location::DB, key3, b"elephant");
		transaction.delete(Location::DB, key1);
		db.write(transaction).unwrap();
		assert_eq!(&*db.get(Location::DB, key3).unwrap().unwrap(), b"elephant");
	}

	#[test]
	fn kvdb() {
		let path = RandomTempPath::create_dir();
		let _ = Database::open_default(path.as_path().to_str().unwrap()).unwrap();
		test_db(&DatabaseConfig::default());
	}
}
