use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct GetAddr;

impl PayloadType for GetAddr {
	fn version() -> u32 {
		60002
	}

	fn command() -> &'static str {
		"getaddr"
	}

	fn deserialize_payload(_reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		Ok(GetAddr)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
