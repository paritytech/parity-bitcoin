use ser::Stream;
use bytes::TaggedBytes;
use common::Magic;
use serialization::serialize_payload;
use {PayloadType, MessageResult, MessageHeader};

pub struct Message<T> {
	bytes: TaggedBytes<T>,
}

impl<T> Message<T> where T: PayloadType {
	pub fn new(magic: Magic, version: u32, payload: &T) -> MessageResult<Self> {
		let serialized = try!(serialize_payload(payload, version));
		let header = MessageHeader::for_data(magic, T::command().into(), &serialized);
		let mut stream = Stream::default();
		stream.append(&header);
		stream.append_slice(&serialized);

		let message = Message {
			bytes: TaggedBytes::new(stream.out()),
		};

		Ok(message)
	}
}

impl<T> AsRef<[u8]> for Message<T> {
	fn as_ref(&self) -> &[u8] {
		self.bytes.as_ref()
	}
}
