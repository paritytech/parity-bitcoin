use std::io;
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

	fn deserialize_payload<T>(_reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		Ok(SendHeaders)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
