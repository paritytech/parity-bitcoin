use std::io;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError,
};
use common::NetAddress;
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub enum Addr {
	V0(V0),
	V31402(V31402),
}

impl Addr {
	pub fn new(addresses: Vec<AddressEntry>) -> Self {
		Addr::V31402(V31402 {
			addresses: addresses,
		})
	}
}

impl Payload for Addr {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"addr"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, version: u32) -> MessageResult<Self> where T: io::Read {
		let result = if version < 31402 {
			reader.read().map(Addr::V0)
		} else {
			reader.read().map(Addr::V31402)
		};

		result.map_err(Into::into)
	}

	fn serialize_payload(&self, stream: &mut Stream, version: u32) -> MessageResult<()> {
		match *self {
			Addr::V0(ref addr) => addr.serialize(stream),
			Addr::V31402(ref addr) => {
				if version < 31402 {
					let view = V31402AsV0::new(addr);
					view.serialize(stream);
				} else {
					addr.serialize(stream);
				}
			}
		}
		Ok(())
	}
}

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
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let entry = AddressEntry {
			timestamp: try!(reader.read()),
			address: try!(reader.read()),
		};

		Ok(entry)
	}
}

#[derive(Debug, PartialEq)]
pub struct V31402 {
	pub addresses: Vec<AddressEntry>,
}

impl Serializable for V31402 {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.addresses);
	}
}

impl Deserializable for V31402 {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = V31402 {
			addresses: try!(reader.read_list_max(1000)),
		};

		Ok(result)
	}
}

#[derive(Debug, PartialEq)]
pub struct V0 {
	pub addresses: Vec<NetAddress>,
}

impl Serializable for V0 {
	fn serialize(&self, stream: &mut Stream) {
		stream.append_list(&self.addresses);
	}
}

impl Deserializable for V0 {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = V0 {
			addresses: try!(reader.read_list_max(1000)),
		};

		Ok(result)
	}
}

struct V31402AsV0<'a> {
	v: &'a V31402,
}

impl<'a> V31402AsV0<'a> {
	fn new(v: &'a V31402) -> Self {
		V31402AsV0 {
			v: v,
		}
	}
}

impl<'a> Serializable for V31402AsV0<'a> {
	fn serialize(&self, stream: &mut Stream) {
		let vec_ref: Vec<&'a NetAddress> = self.v.addresses.iter().map(|x| &x.address).collect();
		stream.append_list::<NetAddress, &'a NetAddress>(&vec_ref);
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::{V31402, AddressEntry};

	#[test]
	fn test_addr_serialize() {
		let expected: Bytes = "01e215104d010000000000000000000000000000000000ffff0a000001208d".into();
		let addr = V31402 {
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
		let expected = V31402 {
			addresses: vec![
				AddressEntry {
					timestamp: 0x4d1015e2,
					address: "010000000000000000000000000000000000ffff0a000001208d".into(),
				},
			],
		};

		assert_eq!(expected, deserialize(raw.as_ref()).unwrap());
	}
}

