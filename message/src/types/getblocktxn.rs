use ser::{Stream, Reader};
use common::BlockTransactionsRequest;
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct GetBlockTxn {
	request: BlockTransactionsRequest,
}

impl PayloadType for GetBlockTxn {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"getblocktxn"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
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
