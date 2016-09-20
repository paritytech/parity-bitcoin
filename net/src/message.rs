use crypto::checksum;
use ser::{
	Deserializable, Reader, Error as ReaderError,
	Serializable, Stream, serialize
};
use {MessageHeader, Payload};

#[derive(Debug, PartialEq)]
pub struct Message {
	pub header: MessageHeader,
	pub payload: Payload,
}

impl Message {
	pub fn new(magic: u32, payload: Payload) -> Message {
		let serialized = serialize(&payload);
		Message {
			header: MessageHeader {
				magic: magic,
				command: payload.command(),
				len: serialized.len() as u32,
				checksum: checksum(&serialized),
			},
			payload: payload,
		}
	}
}

impl Serializable for Message {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.header)
			.append(&self.payload);
	}
}

impl Deserializable for Message {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let header: MessageHeader = try!(reader.read());
		let payload = try!(reader.read_slice(header.len as usize));

		if header.checksum != checksum(payload) {
			return Err(ReaderError::MalformedData);
		}

		let mut payload_reader = Reader::new(payload);
		let payload = try!(Payload::deserialize_payload(&mut payload_reader, &header.command));

		let message = Message {
			header: header,
			payload: payload,
		};

		Ok(message)
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use {Version, Payload};
	use super::Message;

	#[test]
	fn test_message_serialization() {
		let expected: Bytes = "f9beb4d976657273696f6e000000000064000000358d493262ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let version: Version = "62ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let magic = 0xd9b4bef9;
		let message = Message::new(magic, Payload::Version(version));
		assert_eq!(serialize(&message), expected);
	}

	#[test]
	fn test_message_deserialization() {
		let raw: Bytes = "f9beb4d976657273696f6e000000000064000000358d493262ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let version: Version = "62ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let magic = 0xd9b4bef9;
		let expected = Message::new(magic, Payload::Version(version));
		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}
