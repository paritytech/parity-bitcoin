use std::io;
use ser::{Stream, Reader};
use common::BlockHeaderAndIDs;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct CompactBlock {
	pub header: BlockHeaderAndIDs,
}

impl Payload for CompactBlock {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"cmpctblock"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let block = CompactBlock {
			header: try!(reader.read()),
		};

		Ok(block)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.header);
		Ok(())
	}
}
