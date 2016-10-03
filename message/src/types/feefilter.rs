use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct FeeFilter {
	fee_rate: u64,
}

impl Serializable for FeeFilter {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.fee_rate);
	}
}

impl Deserializable for FeeFilter {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let fee_filter = FeeFilter {
			fee_rate: try!(reader.read()),
		};

		Ok(fee_filter)
	}
}
