use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use common::InventoryVector;

#[derive(Debug, PartialEq)]
pub struct Inv {
	pub inventory: Vec<InventoryVector>,
}

impl Serializable for Inv {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.inventory);
	}
}

impl Deserializable for Inv {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let inv = Inv {
			inventory: try!(reader.read_list()),
		};

		Ok(inv)
	}
}
