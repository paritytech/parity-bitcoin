use bytes::Bytes;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct FilterLoad {
	// TODO: check how this should be serialized
	filter: Bytes,
	hash_functions: u32,
	tweak: u32,
	flags: u8,
}

impl Serializable for FilterLoad {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.filter)
			.append(&self.hash_functions)
			.append(&self.tweak)
			.append(&self.flags);
	}
}

impl Deserializable for FilterLoad {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let filterload = FilterLoad {
			filter: try!(reader.read()),
			hash_functions: try!(reader.read()),
			tweak: try!(reader.read()),
			flags: try!(reader.read()),
		};

		Ok(filterload)
	}
}
