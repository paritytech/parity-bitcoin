use std::io;
use hash::H256;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum InventoryType {
	Error = 0,
	MessageTx = 1,
	MessageBlock = 2,
	MessageFilteredBlock = 3,
	MessageCompactBlock = 4,
}

impl InventoryType {
	pub fn from_u32(v: u32) -> Option<Self> {
		match v {
			0 => Some(InventoryType::Error),
			1 => Some(InventoryType::MessageTx),
			2 => Some(InventoryType::MessageBlock),
			3 => Some(InventoryType::MessageFilteredBlock),
			4 => Some(InventoryType::MessageCompactBlock),
			_ => None
		}
	}
}

impl From<InventoryType> for u32 {
	fn from(i: InventoryType) -> Self {
		i as u32
	}
}

#[derive(Debug, PartialEq, Clone)]
pub struct InventoryVector {
	pub inv_type: u32, // TODO: change to InventoryType as discussed in #37
	pub hash: H256,
}

impl Serializable for InventoryVector {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.inv_type)
			.append(&self.hash);
	}
}

impl Deserializable for InventoryVector {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let vec = InventoryVector {
			inv_type: try!(reader.read()),
			hash: try!(reader.read()),
		};

		Ok(vec)
	}
}

impl InventoryVector {
	pub fn inventory_type(&self) -> Option<InventoryType> {
		InventoryType::from_u32(self.inv_type)
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::{InventoryVector, InventoryType};

	#[test]
	fn test_inventory_serialize() {
		let expected = "020000000000000000000000000000000000000000000000000000000000000000000004".into();

		let inventory = InventoryVector {
			inv_type: 2,
			hash: 4u8.into(),
		};

		assert_eq!(serialize(&inventory), expected);
	}

	#[test]
	fn test_inventory_deserialize() {
		let raw: Bytes = "020000000000000000000000000000000000000000000000000000000000000000000004".into();

		let expected = InventoryVector {
			inv_type: 2,
			hash: 4u8.into(),
		};

		assert_eq!(expected, deserialize(raw.as_ref()).unwrap());
	}

	#[test]
	fn test_inventory_type_conversion() {
		assert_eq!(0u32, InventoryType::Error.into());
		assert_eq!(1u32, InventoryType::MessageTx.into());
		assert_eq!(2u32, InventoryType::MessageBlock.into());
		assert_eq!(3u32, InventoryType::MessageFilteredBlock.into());
		assert_eq!(4u32, InventoryType::MessageCompactBlock.into());

		assert_eq!(InventoryType::from_u32(0).unwrap(), InventoryType::Error);
		assert_eq!(InventoryType::from_u32(1).unwrap(), InventoryType::MessageTx);
		assert_eq!(InventoryType::from_u32(2).unwrap(), InventoryType::MessageBlock);
		assert_eq!(InventoryType::from_u32(3).unwrap(), InventoryType::MessageFilteredBlock);
		assert_eq!(InventoryType::from_u32(4).unwrap(), InventoryType::MessageCompactBlock);
	}
}
