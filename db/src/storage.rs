//! Bitcoin storage

use kvdb::Database;
use primitives::hash::H256;
use super::{BlockRef, Bytes};
use byteorder::{LittleEndian, ByteOrder};
use std::{self, fs};
use std::path::Path;

const COL_COUNT: u32 = 10;
const COL_META: u32 = 0;
const COL_BLOCK_HASHES: u32 = 1;
const COL_BLOCK_HEADERS: u32 = 2;
const COL_BLOCK_TRANSACTIONS: u32 = 3;
const COL_TRANSACTIONS: u32 = 4;
//const COL_RESERVED1: u32 = 5;
//const COL_RESERVED2: u32 = 6;
//const COL_RESERVED3: u32 = 7;
//const COL_RESERVED4: u32 = 8;
//const COL_RESERVED5: u32 = 9;
//const COL_RESERVED6: u32 = 10;

pub trait Store {
	fn block_hash(&self, number: u64) -> Option<H256>;

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<H256>;

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;
}

struct Storage {
	database: Database,
}

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

		Ok(Storage {
			database: try!(Database::open_default(&*path.as_ref().to_string_lossy())),
		})
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
		Vec::new()
	}

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> { None }
}

#[cfg(test)]
mod tests {


}
