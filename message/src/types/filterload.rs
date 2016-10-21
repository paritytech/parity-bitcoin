use bytes::Bytes;
use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct FilterLoad {
	// TODO: check how this should be serialized
	pub filter: Bytes,
	pub hash_functions: u32,
	pub tweak: u32,
	pub flags: u8,
}

impl Payload for FilterLoad {
	fn version() -> u32 {
		70001
	}

	fn command() -> &'static str {
		"filterload"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let filterload = FilterLoad {
			filter: try!(reader.read()),
			hash_functions: try!(reader.read()),
			tweak: try!(reader.read()),
			flags: try!(reader.read()),
		};

		Ok(filterload)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.filter)
			.append(&self.hash_functions)
			.append(&self.tweak)
			.append(&self.flags);
		Ok(())
	}
}
