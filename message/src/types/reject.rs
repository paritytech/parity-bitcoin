use std::io;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use {Payload, MessageResult};

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum RejectCode {
	Malformed = 0x01,
	Invalid = 0x10,
	Obsolate = 0x11,
	Duplicate = 0x12,
	Nonstandard = 0x40,
	Dust = 0x41,
	InsuficientFee = 0x42,
	Checkpoint = 0x43,
}

impl From<RejectCode> for u8 {
	fn from(c: RejectCode) -> Self {
		c as u8
	}
}

impl RejectCode {
	pub fn from_u8(v: u8) -> Option<Self> {
		let some = match v {
			0x01 => RejectCode::Malformed,
			0x10 => RejectCode::Invalid,
			0x11 => RejectCode::Obsolate,
			0x12 => RejectCode::Duplicate,
			0x40 => RejectCode::Nonstandard,
			0x41 => RejectCode::Dust,
			0x42 => RejectCode::InsuficientFee,
			0x43 => RejectCode::Checkpoint,
			_ => return None,
		};

		Some(some)
	}
}

impl Serializable for RejectCode {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u8::from(*self));
	}
}

impl Deserializable for RejectCode {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let v: u8 = try!(reader.read());
		RejectCode::from_u8(v).ok_or_else(|| ReaderError::MalformedData)
	}
}

#[derive(Debug, PartialEq)]
pub struct Reject {
	pub message: String,
	pub code: RejectCode,
	pub reason: String,
	// TODO: data
}

impl Payload for Reject {
	fn version() -> u32 {
		0
	}

	fn command() -> &'static str {
		"reject"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let reject = Reject {
			message: try!(reader.read()),
			code: try!(reader.read()),
			reason: try!(reader.read()),
		};

		Ok(reject)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.message)
			.append(&self.code)
			.append(&self.reason);
		Ok(())
	}
}
