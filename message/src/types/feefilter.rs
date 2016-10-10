use ser::{Stream, Reader};
use {PayloadType, MessageResult};

#[derive(Debug, PartialEq)]
pub struct FeeFilter {
	fee_rate: u64,
}

impl PayloadType for FeeFilter {
	fn version() -> u32 {
		70013
	}

	fn command() -> &'static str {
		"cmpctblock"
	}

	fn deserialize_payload(reader: &mut Reader, _version: u32) -> MessageResult<Self> where Self: Sized {
		let fee_filter = FeeFilter {
			fee_rate: try!(reader.read()),
		};

		Ok(fee_filter)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream.append(&self.fee_rate);
		Ok(())
	}
}
