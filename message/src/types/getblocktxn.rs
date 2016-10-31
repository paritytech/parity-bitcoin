use std::io;
use ser::{Stream, Reader};
use common::BlockTransactionsRequest;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct GetBlockTxn {
	pub request: BlockTransactionsRequest,
}

impl Payload for GetBlockTxn {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"getblocktxn"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let get_block = GetBlockTxn {
			request: try!(reader.read()),
		};

		Ok(get_block)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.request);
		Ok(())
	}
}
