use bytes::Bytes;
use crypto::checksum;
use ser::{
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};
use Command;

#[derive(Debug, PartialEq)]
pub struct Message {
	pub magic: u32,
	pub command: Command,
	pub payload: Bytes,
}

impl Message {
	#[inline]
	pub fn payload_checksum(&self) -> [u8; 4] {
		checksum(&self.payload)
	}
}

impl Serializable for Message {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.magic)
			.append(&self.command)
			.append(&(self.payload.len() as u32))
			.append_slice(&self.payload_checksum())
			.append_slice(&self.payload);
	}
}

impl Deserializable for Message {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let magic = try!(reader.read());
		let command = try!(reader.read());
		let len: u32 = try!(reader.read());
		let cs = try!(reader.read_slice(4));
		let payload = try!(reader.read_slice(len as usize));

		if cs != &checksum(payload) {
			return Err(ReaderError::MalformedData);
		}

		let message = Message {
			magic: magic,
			command: command,
			payload: payload.to_vec().into(),
		};

		Ok(message)
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::{serialize, deserialize};
	use super::Message;

	#[test]
	fn test_message_serialization() {
		let expected: Bytes = "f9beb4d976657273696f6e000000000064000000358d493262ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let payload: Bytes = "62ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();

		let message = Message {
			magic: 0xd9b4bef9,
			command: "version".into(),
			payload: payload,
		};

		assert_eq!(serialize(&message), expected);
	}

	#[test]
	fn test_message_deserialization() {
		let raw: Bytes = "f9beb4d976657273696f6e000000000064000000358d493262ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let payload: Bytes = "62ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();

		let expected = Message {
			magic: 0xd9b4bef9,
			command: "version".into(),
			payload: payload,
		};

		assert_eq!(expected, deserialize(&raw).unwrap());
	}
}
