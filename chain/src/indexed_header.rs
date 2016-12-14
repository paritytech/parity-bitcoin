use std::{io, cmp};
use hash::H256;
use ser::{Deserializable, Reader, Error as ReaderError};
use block_header::BlockHeader;
use read_and_hash::ReadAndHash;

#[derive(Debug, Clone)]
pub struct IndexedBlockHeader {
	pub hash: H256,
	pub raw: BlockHeader,
}

impl From<BlockHeader> for IndexedBlockHeader {
	fn from(header: BlockHeader) -> Self {
		IndexedBlockHeader {
			hash: header.hash(),
			raw: header,
		}
	}
}

impl IndexedBlockHeader {
	pub fn new(hash: H256, header: BlockHeader) -> Self {
		IndexedBlockHeader {
			hash: hash,
			raw: header,
		}
	}
}

impl cmp::PartialEq for IndexedBlockHeader {
	fn eq(&self, other: &Self) -> bool {
		self.hash == other.hash
	}
}

impl Deserializable for IndexedBlockHeader {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let data = try!(reader.read_and_hash::<BlockHeader>());
		// TODO: use len
		let header = IndexedBlockHeader {
			raw: data.data,
			hash: data.hash,
		};

		Ok(header)
	}
}
