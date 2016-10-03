use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct SendCompact {
	first: bool,
	second: u64,
}

impl Serializable for SendCompact {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.first)
			.append(&self.second);
	}
}

impl Deserializable for SendCompact {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let send_compact = SendCompact {
			first: try!(reader.read()),
			second: try!(reader.read()),
		};

		Ok(send_compact)
	}
}
