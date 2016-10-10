use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Pong {
	pub nonce: u64,
}

impl PayloadType for Pong {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"pong"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let pong = Pong {
			nonce: try!(reader.read()),
		};

		Ok(pong)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.nonce);
		Ok(())
	}
}
