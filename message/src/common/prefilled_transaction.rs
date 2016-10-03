use ser::{
	Serializable, Stream, CompactInteger,
	Deserializable, Reader, Error as ReaderError
};
use chain::Transaction;

#[derive(Debug, PartialEq)]
pub struct PrefilledTransaction {
	index: usize,
	transaction: Transaction,
}

impl Serializable for PrefilledTransaction {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&CompactInteger::from(self.index))
			.append(&self.transaction);
	}
}

impl Deserializable for PrefilledTransaction {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let compact: CompactInteger = try!(reader.read());
		let tx = PrefilledTransaction {
			index: compact.into(),
			transaction: try!(reader.read()),
		};

		Ok(tx)
	}
}
