use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use serialization::PayloadType;

#[derive(Debug, PartialEq)]
pub struct Verack;

impl Serializable for Verack {
	fn serialize(&self, _stream: &mut Stream) {}
}

impl Deserializable for Verack {
	fn deserialize(_reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		Ok(Verack)
	}
}

impl PayloadType for Verack {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"verack"
	}
}
