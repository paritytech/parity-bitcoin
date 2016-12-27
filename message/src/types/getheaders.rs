use std::io;
use hash::H256;
use ser::{Stream, Reader};
use {Payload, MessageResult};

pub const GETHEADERS_MAX_RESPONSE_HEADERS: usize = 2_000;

#[derive(Debug, PartialEq)]
pub struct GetHeaders {
	pub version: u32,
	pub block_locator_hashes: Vec<H256>,
	pub hash_stop: H256,
}

impl GetHeaders {
	pub fn with_block_locator_hashes(block_locator_hashes: Vec<H256>) -> Self {
		GetHeaders {
			version: 0, // this field is ignored by implementations
			block_locator_hashes: block_locator_hashes,
			hash_stop: H256::default(),
		}
	}
}

impl Payload for GetHeaders {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"getheaders"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let get_blocks = GetHeaders {
			version: try!(reader.read()),
			block_locator_hashes: try!(reader.read_list_max(2000)),
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

