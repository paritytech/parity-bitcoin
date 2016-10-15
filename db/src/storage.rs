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
	fn block_hash(&self, number: u64) -> Option<H256>;

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<H256>;

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	fn block(&self, block_ref: BlockRef) -> Option<chain::Block>;
}

pub struct Storage {
	database: Database,
}

#[derive(Debug)]
pub enum MetaError {
	NoVersion,
	UnsupportedVersion,
}

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

impl Storage {

	// new storage at the selected path
	// if no directory exists, it will be created
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
			_ => Err(Error::Meta(MetaError::NoVersion))
		}
	}

	fn db_error(&self, msg: String) {
		println!("Low-level database error: {}", &msg);
	}

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

	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}

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
										self.db_error(format!("Error deserializing header, possible db corruption ({:?})", e));
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
}

#[cfg(test)]
mod tests {
}
