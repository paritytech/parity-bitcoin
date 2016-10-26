//! Bitcoin storage

use std::{self, fs};
use std::path::Path;
use kvdb::{Database, DatabaseConfig};
use byteorder::{LittleEndian, ByteOrder};
use primitives::hash::H256;
use primitives::bytes::Bytes;
use super::BlockRef;
use serialization;
use chain::{self, RepresentH256};
use parking_lot::RwLock;

const COL_COUNT: u32 = 10;
const COL_META: u32 = 0;
const COL_BLOCK_HASHES: u32 = 1;
const COL_BLOCK_HEADERS: u32 = 2;
const COL_BLOCK_TRANSACTIONS: u32 = 3;
const COL_TRANSACTIONS: u32 = 4;
const _COL_RESERVED1: u32 = 5;
const _COL_RESERVED2: u32 = 6;
const _COL_RESERVED3: u32 = 7;
const _COL_RESERVED4: u32 = 8;
const _COL_RESERVED5: u32 = 9;
const _COL_RESERVED6: u32 = 10;

const DB_VERSION: u32 = 1;

/// Blockchain storage interface
pub trait Store : Send + Sync {
	/// get best block number
	fn best_block_number(&self) -> Option<u32>;

	/// get best block hash
	fn best_block_hash(&self) -> Option<H256>;

	/// resolves hash by block number
	fn block_hash(&self, number: u32) -> Option<H256>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	/// resolves list of block transactions by block reference (number/hash)
	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256>;

	/// resolves transaction body bytes by transaction hash
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// resolves serialized transaction info by transaction hash
	fn transaction(&self, hash: &H256) -> Option<chain::Transaction>;

	/// returns all transactions in the block by block reference (number/hash)
	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction>;

	/// resolves deserialized block body by block reference (number/hash)
	fn block(&self, block_ref: BlockRef) -> Option<chain::Block>;

	/// returns true if store contains given block
	fn contains_block(&self, block_ref: BlockRef) -> bool {
		self.block_header_bytes(block_ref).is_some()
	}

	/// insert block in the storage
	fn insert_block(&self, block: &chain::Block) -> Result<(), Error>;
}

/// Blockchain storage with rocksdb database
pub struct Storage {
	database: Database,
	best_block_number: RwLock<Option<u32>>,
	best_block_hash: RwLock<Option<H256>>,
}

#[derive(Debug)]
pub enum MetaError {
	UnsupportedVersion,
}

#[derive(Debug)]
/// Database error
pub enum Error {
	/// Rocksdb error
	DB(String),
	/// Io error
	Io(std::io::Error),
	/// Invalid meta info
	Meta(MetaError),
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

fn u32_key(num: u32) -> [u8; 4] {
	let mut result = [0u8; 4];
	LittleEndian::write_u32(&mut result, num);
	result
}

const KEY_VERSION: &'static[u8] = b"version";
const KEY_BEST_BLOCK_NUMBER: &'static[u8] = b"best_block_number";
const KEY_BEST_BLOCK_HASH: &'static[u8] = b"best_block_hash";

impl Storage {

	/// new storage at the selected path
	/// if no directory exists, it will be created
	pub fn new<P: AsRef<Path>>(path: P) -> Result<Storage, Error> {
		try!(fs::create_dir_all(path.as_ref()));
		let cfg = DatabaseConfig::with_columns(Some(COL_COUNT));
		let db = try!(Database::open(&cfg, &*path.as_ref().to_string_lossy()));

		let storage = Storage {
			database: db,
			best_block_number: RwLock::default(),
			best_block_hash: RwLock::default(),
		};

		match storage.read_meta_u32(KEY_VERSION) {
			Some(ver) => {
				if ver != DB_VERSION {
					return Err(Error::Meta(MetaError::UnsupportedVersion))
				}
			},
			_ => {
				let mut meta_transaction = storage.database.transaction();
				meta_transaction.write_u32(Some(COL_META), KEY_VERSION, DB_VERSION);
				try!(storage.database.write(meta_transaction));
			},
		};

		*storage.best_block_number.write() = storage.read_meta_u32(KEY_BEST_BLOCK_NUMBER);
		*storage.best_block_hash.write() = storage.get(COL_META, KEY_BEST_BLOCK_HASH).map(|val| H256::from(&**val));

		Ok(storage)
	}

	fn read_meta(&self, key: &[u8]) -> Option<Bytes> {
		self.get(COL_META, key)
	}

	fn read_meta_u64(&self, key: &[u8]) -> Option<u64> {
		self.read_meta(key).map(|val| LittleEndian::read_u64(&val))
	}

	fn read_meta_u32(&self, key: &[u8]) -> Option<u32> {
		self.read_meta(key).map(|val| LittleEndian::read_u32(&val))
	}

	/// is invoked on database non-fatal query errors
	fn db_error(&self, msg: String) {
		println!("Low-level database error: {}", &msg);
	}

	/// get the value of the key in the database
	/// if the key is not present, reports non-fatal error and returns nothing
	fn get(&self, col: u32, key: &[u8]) -> Option<Bytes> {
		let res = self.database.get(Some(col), key);
		match res {
			Err(msg) => {
				self.db_error(msg);
				None
			},
			Ok(val) => val.map(|v| v.into()),
		}
	}

	/// resolves hash for the block reference (which can be referenced by number or
	/// by hash)
	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}

	/// loads block transaction list by the provided block hash
	fn block_transaction_hashes_by_hash(&self, h: &H256) -> Vec<H256> {
		self.get(COL_BLOCK_TRANSACTIONS, &**h)
			.unwrap_or(Vec::new().into())
			.chunks(H256::size())
			.map(H256::from)
			.collect()
	}

	fn block_transactions_by_hash(&self, h: &H256) -> Vec<chain::Transaction> {
		self.block_transaction_hashes_by_hash(h)
			.into_iter()
			.filter_map(|tx_hash| {
				self.transaction_bytes(&tx_hash).and_then(|tx_bytes| {
					match serialization::deserialize::<chain::Transaction>(&tx_bytes) {
						Ok(tx) => Some(tx),
						Err(e) => {
							self.db_error(format!("Error deserializing transaction, possible db corruption ({:?})", e));
							None
						}
					}
				})
			})
			.collect()
	}
}

impl Store for Storage {
	fn best_block_number(&self) -> Option<u32> {
		*self.best_block_number.read()
	}

	fn best_block_hash(&self) -> Option<H256> {
		self.best_block_hash.read().clone()
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		self.get(COL_BLOCK_HASHES, &u32_key(number))
			.map(|val| H256::from(&**val))
	}

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.resolve_hash(block_ref).and_then(|h| self.get(COL_BLOCK_HEADERS, &*h))
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transaction_hashes_by_hash(&h))
			.unwrap_or(Vec::new())
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transactions_by_hash(&h))
			.unwrap_or(Vec::new())
	}

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.get(COL_TRANSACTIONS, &**hash)
	}

	fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
		self.resolve_hash(block_ref).and_then(|block_hash|
			self.get(COL_BLOCK_HEADERS, &*block_hash)
				.and_then(|header_bytes| {
					let transactions = self.block_transactions_by_hash(&block_hash);;
					let maybe_header = match serialization::deserialize::<chain::BlockHeader>(&header_bytes) {
						Ok(header) => Some(header),
						Err(e) => {
							self.db_error(format!("Error deserializing header, possible db corruption ({:?})", e));
							None
						}
					};
					maybe_header.map(|header| chain::Block::new(header, transactions))
			})
		)
	}

	fn insert_block(&self, block: &chain::Block) -> Result<(), Error> {
		let mut best_block_number = self.best_block_number.write();
		let mut best_block_hash = self.best_block_hash.write();

		let block_hash = block.hash();

		let new_best_hash = match best_block_hash.as_ref() {
			Some(best_hash) => {
				if &block.header().previous_header_hash == best_hash {
					block_hash.clone()
				}
				else {
					best_hash.clone()
				}
			},
			None => { block_hash.clone() },
		};

		let new_best_number = match *best_block_number {
			Some(best_number) => {
				if block.hash() == new_best_hash { best_number + 1 }
				else { best_number }
			},
			None => 1,
		};

		let mut transaction = self.database.transaction();

		let tx_space = block.transactions().len() * 32;
		let mut tx_refs = Vec::with_capacity(tx_space);
		for tx in block.transactions() {
			let tx_hash = tx.hash();
			tx_refs.extend(&*tx_hash);
			transaction.put(
				Some(COL_TRANSACTIONS),
				&*tx_hash,
				&serialization::serialize(tx),
			);
		}
		transaction.put(Some(COL_BLOCK_TRANSACTIONS), &*block_hash, &tx_refs);

		transaction.put(
			Some(COL_BLOCK_HEADERS),
			&*block_hash,
			&serialization::serialize(block.header())
		);

		if *best_block_number != Some(new_best_number) {
			transaction.write_u32(Some(COL_META), KEY_BEST_BLOCK_NUMBER, new_best_number);
		}

		if best_block_hash.as_ref() != Some(&new_best_hash) {
			transaction.put(Some(COL_META), KEY_BEST_BLOCK_HASH, std::ops::Deref::deref(&new_best_hash));
		}

		try!(self.database.write(transaction));

		*best_block_number = Some(new_best_number);
		*best_block_hash = Some(new_best_hash);

		Ok(())
	}

	fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
		self.transaction_bytes(hash).and_then(|tx_bytes| {
			serialization::deserialize(&tx_bytes).map_err(
				|e| self.db_error(format!("Error deserializing transaction, possible db corruption ({:?})", e))
			).ok()
		})
	}

}

#[cfg(test)]
mod tests {

	use super::{Storage, Store};
	use devtools::RandomTempPath;
	use chain::{Block, RepresentH256};
	use super::super::BlockRef;
	use test_data;

	#[test]
	fn open_store() {
		let path = RandomTempPath::create_dir();
		assert!(Storage::new(path.as_path()).is_ok());
	}

	#[test]
	fn insert_block() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block1();
		store.insert_block(&block).unwrap();

		let loaded_block = store.block(BlockRef::Hash(block.hash())).unwrap();
		assert_eq!(loaded_block.hash(), block.hash());
	}

	#[test]
	fn best_block_update() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block1();
		store.insert_block(&block).unwrap();

		assert_eq!(store.best_block_number(), Some(1));
	}

	#[test]
	fn best_hash_update_fork() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block1();
		store.insert_block(&block).unwrap();

		let another_block: Block = test_data::block_h169();
		store.insert_block(&another_block).unwrap();

		// did not update because `another_block` is not child of `block`
		assert_eq!(store.best_block_hash(), Some(block.hash()));
		// number should not be update also
		assert_eq!(store.best_block_number(), Some(1));
	}

	#[test]
	fn load_transaction() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block1();
		let tx1 = block.transactions()[0].hash();
		store.insert_block(&block).unwrap();

		let loaded_transaction = store.transaction(&tx1).unwrap();
		assert_eq!(loaded_transaction.hash(), block.transactions()[0].hash());
	}

}
