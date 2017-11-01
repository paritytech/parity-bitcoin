use ser::Stream;
use bytes::{TaggedBytes, Bytes};
use network::Magic;
use common::Command;
use serialization::serialize_payload_with_flags;
use {Payload, MessageResult, MessageHeader};

pub fn to_raw_message(magic: Magic, command: Command, payload: &Bytes) -> Bytes {
	let header = MessageHeader::for_data(magic, command, payload);
	let mut stream = Stream::default();
	stream.append(&header);
	stream.append_slice(payload);
	stream.out()
}

pub struct Message<T> {
	bytes: TaggedBytes<T>,
}

impl<T> Message<T> where T: Payload {
	pub fn new(magic: Magic, version: u32, payload: &T) -> MessageResult<Self> {
		Self::with_flags(magic, version, payload, 0)
	}

	pub fn with_flags(magic: Magic, version: u32, payload: &T, serialization_flags: u32) -> MessageResult<Self> {
		let serialized = try!(serialize_payload_with_flags(payload, version, serialization_flags));

		let message = Message {
			bytes: TaggedBytes::new(to_raw_message(magic, T::command().into(), &serialized)),
		};

		Ok(message)
	}

	pub fn len(&self) -> usize {
		self.bytes.len()
	}
}

impl<T> AsRef<[u8]> for Message<T> {
	fn as_ref(&self) -> &[u8] {
		self.bytes.as_ref()
	}
}

impl<T> From<Message<T>> for Bytes {
	fn from(m: Message<T>) -> Self {
		m.bytes.into_raw()
	}
}
