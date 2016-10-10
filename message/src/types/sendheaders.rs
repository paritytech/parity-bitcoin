use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct SendHeaders;

impl PayloadType for SendHeaders {
	fn version() -> u32 {
		70012
	}

	fn command() -> &'static str {
		"sendheaders"
	}

	fn deserialize_payload(_reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		Ok(SendHeaders)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
