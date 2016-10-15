use hash::H256;
use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct GetBlocks {
	version: u32,
	block_locator_hashes: Vec<H256>,
	hash_stop: H256,
}

impl Payload for GetBlocks {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"getblocks"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let get_blocks = GetBlocks {
			version: try!(reader.read()),
			block_locator_hashes: try!(reader.read_list_max(500)),
			hash_stop: try!(reader.read()),
		};

		Ok(get_blocks)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.version)
			.append_list(&self.block_locator_hashes)
			.append(&self.hash_stop);
		Ok(())
	}
}

