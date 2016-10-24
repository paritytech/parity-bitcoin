use ser::{Stream, Reader};
use chain::Transaction;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Tx {
	pub transaction: Transaction,
}

impl Payload for Tx {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"tx"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let tx = Tx {
			transaction: try!(reader.read()),
		};

		Ok(tx)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.transaction);
		Ok(())
	}
}
