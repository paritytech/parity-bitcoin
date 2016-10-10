use chain::BlockHeader;
use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Headers {
	// TODO: Block headers need to have txn_count field
	headers: Vec<BlockHeader>,
}

impl PayloadType for Headers {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"headers"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
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
