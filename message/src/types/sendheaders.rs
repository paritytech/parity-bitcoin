use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct SendHeaders;

impl Payload for SendHeaders {
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
