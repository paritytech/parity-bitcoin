use std::io;
use ser::{Stream, Reader};
use {MessageResult, Payload};

#[derive(Debug, PartialEq)]
pub struct Ping {
	pub nonce: u64,
}

impl Ping {
	pub fn new(nonce: u64) -> Self {
		Ping {
			nonce: nonce,
		}
	}
}

impl Payload for Ping {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"ping"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
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
