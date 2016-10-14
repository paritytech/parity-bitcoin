use ser::{Stream, Reader};
use common::BlockTransactions;
use {MessageResult, Payload};

#[derive(Debug, PartialEq)]
pub struct BlockTxn {
	request: BlockTransactions,
}

impl Payload for BlockTxn {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"blocktxn"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let block = BlockTxn {
			request: try!(reader.read()),
		};

		Ok(block)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.request);
		Ok(())
	}
}
