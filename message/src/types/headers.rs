use chain::BlockHeader;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

pub struct Headers {
	headers: Vec<BlockHeader>,
}

impl Serializable for Headers {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.headers);
	}
}

impl Deserializable for Headers {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let headers = Headers {
			headers: try!(reader.read_list()),
		};

		Ok(headers)
	}
}
