use std::io;
use chain::BlockHeader;
use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Headers {
	// TODO: Block headers need to have txn_count field
	pub headers: Vec<BlockHeader>,
}

impl Payload for Headers {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"headers"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let headers = Headers {
			headers: try!(reader.read_list()),
		};

		Ok(headers)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append_list(&self.headers);
		Ok(())
	}
}
