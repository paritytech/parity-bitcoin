use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct SendCompact {
	pub first: bool,
	pub second: u64,
}

impl Payload for SendCompact {
	fn version() -> u32 {
		70014
	}

	fn command() -> &'static str {
		"sendcmpct"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let send_compact = SendCompact {
			first: try!(reader.read()),
			second: try!(reader.read()),
		};

		Ok(send_compact)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.first)
			.append(&self.second);
		Ok(())
	}
}
