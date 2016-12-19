use std::io;
use ser::{Stream, Reader};
use {Payload, MessageResult};

#[derive(Debug, PartialEq)]
pub struct FeeFilter {
	pub fee_rate: u64,
}

impl FeeFilter {
	pub fn with_fee_rate(fee_rate: u64) -> Self {
		FeeFilter {
			fee_rate: fee_rate,
		}
	}
}

impl Payload for FeeFilter {
	fn version() -> u32 {
		70013
	}

	fn command() -> &'static str {
		"feefilter"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
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
