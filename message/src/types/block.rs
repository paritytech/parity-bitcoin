use ser::{Stream, Reader};
use chain::Block as ChainBlock;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Block {
	pub block: ChainBlock,
}

impl Payload for Block {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"block"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let tx = Block {
			block: try!(reader.read()),
		};

		Ok(tx)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.block);
		Ok(())
	}
}
