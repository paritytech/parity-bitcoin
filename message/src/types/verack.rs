use std::io;
use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct Verack;

impl Payload for Verack {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"verack"
	}

	fn deserialize_payload<T>(_reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		Ok(Verack)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
