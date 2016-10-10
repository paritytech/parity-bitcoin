use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use serialization::PayloadType;

#[derive(Debug, PartialEq)]
pub struct Pong {
	pub nonce: u64,
}

impl Serializable for Pong {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.nonce);
	}
}

impl Deserializable for Pong {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let ping = Pong {
			nonce: try!(reader.read()),
		};

		Ok(ping)
	}
}

impl PayloadType for Pong {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"pong"
	}
}
