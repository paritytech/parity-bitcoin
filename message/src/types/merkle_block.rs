use std::io;
use hash::H256;
use bytes::Bytes;
use ser::{Stream, Reader};
use chain::BlockHeader;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct MerkleBlock {
	pub block_header: BlockHeader,
	pub total_transactions: u32,
	pub hashes: Vec<H256>,
	pub flags: Bytes,
}

impl Payload for MerkleBlock {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"merkleblock"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let merkle_block = MerkleBlock {
			block_header: try!(reader.read()),
			total_transactions: try!(reader.read()),
			hashes: try!(reader.read_list()),
			flags: try!(reader.read()),
		};

		Ok(merkle_block)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.block_header)
			.append(&self.total_transactions)
			.append_list(&self.hashes)
			.append(&self.flags);
		Ok(())
	}
}
