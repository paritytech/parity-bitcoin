use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct FilterClear;

impl PayloadType for FilterClear {
	fn version() -> u32 {
		70001
	}

	fn command() -> &'static str {
		"filterclear"
	}

	fn deserialize_payload(_reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		Ok(FilterClear)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
