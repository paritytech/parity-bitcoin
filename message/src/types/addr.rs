use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError,
};
use serialization::PayloadType;
use common::NetAddress;

#[derive(Debug, PartialEq)]
pub struct AddressEntry {
	pub timestamp: u32,
	pub address: NetAddress,
}

impl Serializable for AddressEntry {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.timestamp)
			.append(&self.address);
	}
}

impl Deserializable for AddressEntry {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let entry = AddressEntry {
			timestamp: try!(reader.read()),
			address: try!(reader.read()),
		};

		Ok(entry)
	}
}

#[derive(Debug, PartialEq)]
pub struct Addr {
	pub addresses: Vec<AddressEntry>,
}

impl Serializable for Addr {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.addresses);
	}
}

impl Deserializable for Addr {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		// TODO: limit to 1000
		let result = Addr {
			addresses: try!(reader.read_list()),
		};

		Ok(result)
	}
}

#[derive(Debug, PartialEq)]
pub struct AddrBelow31402 {
	pub addresses: Vec<NetAddress>,
}

impl Serializable for AddrBelow31402 {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.addresses);
	}
}

impl Deserializable for AddrBelow31402 {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		// TODO: limit to 1000
		let result = AddrBelow31402 {
			addresses: try!(reader.read_list()),
		};

		Ok(result)
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::{Addr, AddressEntry};

	#[test]
	fn test_addr_serialize() {
		let expected: Bytes = "01e215104d010000000000000000000000000000000000ffff0a000001208d".into();
		let addr = Addr {
			addresses: vec![
				AddressEntry {
					timestamp: 0x4d1015e2,
					address: "010000000000000000000000000000000000ffff0a000001208d".into(),
				},
			],
		};

		assert_eq!(serialize(&addr), expected);
	}

	#[test]
	fn test_addr_deserialize() {
		let raw: Bytes = "01e215104d010000000000000000000000000000000000ffff0a000001208d".into();
		let expected = Addr {
			addresses: vec![
				AddressEntry {
					timestamp: 0x4d1015e2,
					address: "010000000000000000000000000000000000ffff0a000001208d".into(),
				},
			],
		};

		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}

