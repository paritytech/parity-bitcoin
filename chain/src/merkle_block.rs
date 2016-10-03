use hash::H256;
use bytes::Bytes;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use BlockHeader;

#[derive(Debug, PartialEq)]
pub struct MerkleBlock {
	block_header: BlockHeader,
	total_transactions: u32,
	hashes: Vec<H256>,
	flags: Bytes,
}

impl Serializable for MerkleBlock {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.block_header)
			.append(&self.total_transactions)
			.append_list(&self.hashes)
			.append(&self.flags);
	}
}

impl Deserializable for MerkleBlock {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let merkle_block = MerkleBlock {
			block_header: try!(reader.read()),
			total_transactions: try!(reader.read()),
			hashes: try!(reader.read_list()),
			flags: try!(reader.read()),
		};

		Ok(merkle_block)
	}
}
