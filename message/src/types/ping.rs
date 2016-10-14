use ser::{Stream, Reader};
use {MessageResult, Payload};

#[derive(Debug, PartialEq)]
pub struct Ping {
	pub nonce: u64,
}

impl Payload for Ping {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"ping"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let ping = Ping {
			nonce: try!(reader.read()),
		};

		Ok(ping)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.nonce);
		Ok(())
	}
}
