use crypto::checksum;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};
use common::Command;

#[derive(Debug, PartialEq)]
pub struct MessageHeader {
	pub magic: u32,
	pub command: Command,
	pub len: u32,
	pub checksum: [u8; 4],
}

impl Deserializable for MessageHeader {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let header = MessageHeader {
			magic: try!(reader.read()),
			command: try!(reader.read()),
			len: try!(reader.read()),
			checksum: {
				let slice = try!(reader.read_slice(4));
				let mut checksum = [0u8; 4];
				checksum.copy_from_slice(slice);
				checksum
			},
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
			.append_slice(&self.checksum);
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::MessageHeader;

	#[test]
	fn test_message_header_serialization() {
		let expected = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let header = MessageHeader {
			magic: 0xd9b4bef9,
			command: "addr".into(),
			len: 0x1f,
			checksum: [0xed, 0x52, 0x39, 0x9b],
		};

		assert_eq!(serialize(&header), expected);
	}

	#[test]
	fn test_message_header_deserialization() {
		let raw: Bytes = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let expected = MessageHeader {
			magic: 0xd9b4bef9,
			command: "addr".into(),
			len: 0x1f,
			checksum: [0xed, 0x52, 0x39, 0x9b],
		};

		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}
