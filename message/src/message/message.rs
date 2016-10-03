use crypto::checksum;
use ser::{Serializable, Stream, serialize};
use common::Magic;
use message::{MessageHeader, Payload};

#[derive(Debug, PartialEq)]
pub struct Message {
	pub header: MessageHeader,
	pub payload: Payload,
}

impl Message {
	pub fn new(magic: Magic, payload: Payload) -> Message {
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

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use ser::serialize;
	use common::Magic;
	use types::Version;
	use super::Message;
	use Payload;

	#[test]
	fn test_message_serialization() {
		let expected: Bytes = "f9beb4d976657273696f6e000000000064000000358d493262ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let version: Version = "62ea0000010000000000000011b2d05000000000010000000000000000000000000000000000ffff000000000000000000000000000000000000000000000000ffff0000000000003b2eb35d8ce617650f2f5361746f7368693a302e372e322fc03e0300".into();
		let magic = Magic::Mainnet;
		let message = Message::new(magic, Payload::Version(version));
		assert_eq!(serialize(&message), expected);
	}
}
