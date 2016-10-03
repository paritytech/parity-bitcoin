use bytes::Bytes;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct FilterAdd {
	// TODO: check how this should be serialized
	data: Bytes,
}

impl Serializable for FilterAdd {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.data);
	}
}

impl Deserializable for FilterAdd {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let filteradd= FilterAdd {
			data: try!(reader.read()),
		};

		Ok(filteradd)
	}
}
