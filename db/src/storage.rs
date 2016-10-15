//! Bitcoin storage

use kvdb::{Database, DatabaseConfig};
use primitives::hash::H256;
use super::{BlockRef, Bytes};
use byteorder::{LittleEndian, ByteOrder};
use std::{self, fs};
use std::path::Path;
use chain;
use serialization::{self, Serializable, Deserializable};

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

pub trait Store {
	/// resolves hash by block number
	fn block_hash(&self, number: u64) -> Option<H256>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	/// resolves list of block transactions by block reference (number/hash)
	fn block_transactions(&self, block_ref: BlockRef) -> Vec<H256>;

	/// resolves transaction body bytes by transaction hash
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// resolves serialized transaction info by transaction hash
	fn transaction(&self, hash: &H256) -> Option<chain::Transaction>;

	/// resolves deserialized block body by block reference (number/hash)
	fn block(&self, block_ref: BlockRef) -> Option<chain::Block>;

	/// insert block in the storage
	fn insert_block(&self, block: &chain::Block) -> Result<(), Error>;
}

pub struct Storage {
	database: Database,
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

fn u64_key(num: u64) -> [u8; 8] {
	let mut result = [0u8; 8];
	LittleEndian::write_u64(&mut result[..], num);
	result
}

const KEY_VERSION: &'static[u8] = b"version";

impl Storage {

	/// new storage at the selected path
	/// if no directory exists, it will be created
	pub fn new<P: AsRef<Path>>(path: P) -> Result<Storage, Error> {
		try!(fs::create_dir_all(path.as_ref()));
		let cfg = DatabaseConfig::with_columns(Some(COL_COUNT));
		let db = try!(Database::open(&cfg, &*path.as_ref().to_string_lossy()));

		match try!(db.get(Some(COL_META), b"version")) {
			Some(val) => {
				let ver = LittleEndian::read_u32(&val);
				if ver == DB_VERSION {
					Ok(Storage { database: db, })
				}
				else {
					Err(Error::Meta(MetaError::UnsupportedVersion))
				}
			},
			_ => {
				let mut meta_transaction = db.transaction();
				let mut ver_val = [0u8; 4];
				LittleEndian::write_u32(&mut ver_val, DB_VERSION);
				meta_transaction.put(Some(COL_META), KEY_VERSION, &ver_val);
				try!(db.write(meta_transaction));
				Ok(Storage { database: db, })
			}
		}
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
			Ok(val) => val,
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
	fn block_transactions_by_hash(&self, h: &H256) -> Vec<H256> {
		self.get(COL_BLOCK_TRANSACTIONS, &**h)
			.unwrap_or(Vec::new())
			.chunks(H256::size())
			.map(H256::from)
			.collect()
	}
}

impl Store for Storage {
	fn block_hash(&self, number: u64) -> Option<H256> {
		self.get(COL_BLOCK_HASHES, &u64_key(number))
			.map(|val| H256::from(&val[..]))
	}

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.resolve_hash(block_ref).and_then(|h| self.get(COL_BLOCK_HEADERS, &*h))
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<H256> {
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
					let transactions = self.block_transactions_by_hash(&block_hash)
						.into_iter()
						.filter_map(|tx_hash| {
							self.transaction_bytes(&tx_hash).and_then(|tx_bytes| {
								let mut reader = serialization::Reader::new(&tx_bytes[..]);
								match chain::Transaction::deserialize(&mut reader) {
									Ok(tx) => Some(tx),
									Err(e) => {
										self.db_error(format!("Error deserializing transaction, possible db corruption ({:?})", e));
										None
									}
								}
							})
						})
						.collect();

					let mut reader = serialization::Reader::new(&header_bytes[..]);
					let maybe_header = match chain::BlockHeader::deserialize(&mut reader) {
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
		let mut transaction = self.database.transaction();

		let tx_space = block.transactions().len() * 32;
		let block_hash = block.hash();
		let mut tx_refs = Vec::with_capacity(tx_space);
		for tx in block.transactions() {
			let tx_hash = tx.hash();
			tx_refs.extend(&*tx_hash);
			let mut tx_stream = serialization::Stream::new();
			tx.serialize(&mut tx_stream);
			transaction.put(Some(COL_TRANSACTIONS), &*tx_hash, tx_stream.out().as_slice());
		}
		transaction.put(Some(COL_BLOCK_TRANSACTIONS), &*block_hash,  &tx_refs);

		let mut header_stream = serialization::Stream::new();
		block.header().serialize(&mut header_stream);
		transaction.put(Some(COL_BLOCK_HEADERS), &*block_hash, header_stream.out().as_slice());

		try!(self.database.write(transaction));

		Ok(())
	}

	fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
		
	}

}

#[cfg(test)]
mod tests {

	use super::{Storage, Store};
	use devtools::RandomTempPath;
	use chain::Block;
	use super::super::BlockRef;

	#[test]
	fn open_store() {
		let path = RandomTempPath::create_dir();
		assert!(Storage::new(path.as_path()).is_ok());
	}

	#[test]
	fn insert_block() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = "01000000ba8b9cda965dd8e536670f9ddec10e53aab14b20bacad27b9137190000000000190760b278fe7b8565fda3b968b918d5fd997f993b23674c0af3b6fde300b38f33a5914ce6ed5b1b01e32f570201000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704e6ed5b1b014effffffff0100f2052a01000000434104b68a50eaa0287eff855189f949c1c6e5f58b37c88231373d8a59809cbae83059cc6469d65c665ccfd1cfeb75c6e8e19413bba7fbff9bc762419a76d87b16086eac000000000100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000".into();
		store.insert_block(&block).unwrap();

		let loaded_block = store.block(BlockRef::Hash(block.hash())).unwrap();
		assert_eq!(loaded_block.hash(), block.hash());
	}

	#[test]
	fn load_transaction() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = "01000000ba8b9cda965dd8e536670f9ddec10e53aab14b20bacad27b9137190000000000190760b278fe7b8565fda3b968b918d5fd997f993b23674c0af3b6fde300b38f33a5914ce6ed5b1b01e32f570201000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704e6ed5b1b014effffffff0100f2052a01000000434104b68a50eaa0287eff855189f949c1c6e5f58b37c88231373d8a59809cbae83059cc6469d65c665ccfd1cfeb75c6e8e19413bba7fbff9bc762419a76d87b16086eac000000000100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000".into();
		let tx1 = block.transactions()[0].hash();
		store.insert_block(&block).unwrap();

		let loaded_transaction = store.transaction_bytes(&tx1).unwrap();
		assert_eq!(vec![0u8; 0], loaded_transaction);

	}

}
