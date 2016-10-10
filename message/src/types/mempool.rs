use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct MemPool;

impl PayloadType for MemPool {
	fn version() -> u32 {
		60002
	}

	fn command() -> &'static str {
		"mempool"
	}

	fn deserialize_payload(_reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		Ok(MemPool)
	}

	fn serialize_payload(&self, _stream: &mut Stream, _version: u32) -> MessageResult<()> {
		Ok(())
	}
}
