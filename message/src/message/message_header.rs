use hash::H32;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};
use common::{Command, Magic};

#[derive(Debug, PartialEq)]
pub struct MessageHeader {
	pub magic: Magic,
	pub command: Command,
	pub len: u32,
	pub checksum: H32,
}

impl Deserializable for MessageHeader {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let header = MessageHeader {
			magic: try!(reader.read()),
			command: try!(reader.read()),
			len: try!(reader.read()),
			checksum: try!(reader.read()),
		};

		Ok(header)
	}
}

impl Serializable for MessageHeader {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.magic)
			.append(&self.command)
			.append(&self.len)
			.append(&self.checksum);
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use common::Magic;
	use super::MessageHeader;

	#[test]
	fn test_message_header_serialization() {
		let expected = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let header = MessageHeader {
			magic: Magic::Mainnet,
			command: "addr".into(),
			len: 0x1f,
			checksum: "ed52399b".into(),
		};

		assert_eq!(serialize(&header), expected);
	}

	#[test]
	fn test_message_header_deserialization() {
		let raw: Bytes = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let expected = MessageHeader {
			magic: Magic::Mainnet,
			command: "addr".into(),
			len: 0x1f,
			checksum: "ed52399b".into(),
		};

		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}
