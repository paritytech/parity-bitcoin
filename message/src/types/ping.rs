use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct Ping {
	nonce: u64,
}

impl Serializable for Ping {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.nonce);
	}
}

impl Deserializable for Ping {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let ping = Ping {
			nonce: try!(reader.read()),
		};

		Ok(ping)
	}
}
