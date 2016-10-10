use bytes::Bytes;
use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct FilterAdd {
	// TODO: check how this should be serialized
	data: Bytes,
}

impl PayloadType for FilterAdd {
	fn version() -> u32 {
		70001
	}

	fn command() -> &'static str {
		"filteradd"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let filteradd = FilterAdd {
			data: try!(reader.read()),
		};

		Ok(filteradd)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.data);
		Ok(())
	}
}
