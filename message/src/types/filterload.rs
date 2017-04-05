use std::io;
use bytes::Bytes;
use ser::{Serializable, Deserializable, Stream, Reader, Error as ReaderError};
use {Payload, MessageResult};

pub const FILTERLOAD_MAX_FILTER_LEN: usize = 36_000;
pub const FILTERLOAD_MAX_HASH_FUNCS: usize = 50;

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
/// Controls how the filter is updated after match is found.
pub enum FilterFlags {
	/// Means the filter is not adjusted when a match is found.
	None = 0,
	/// Means if the filter matches any data element in a scriptPubKey the outpoint is serialized and inserted into the filter.
	All = 1,
	/// Means the outpoint is inserted into the filter only if a data element in the scriptPubKey is matched, and that script is
	/// of the standard "pay to pubkey" or "pay to multisig" forms.
	PubKeyOnly = 2,
}

#[derive(Debug, PartialEq)]
pub struct FilterLoad {
	// TODO: check how this should be serialized
	pub filter: Bytes,
	pub hash_functions: u32,
	pub tweak: u32,
	pub flags: FilterFlags,
}

impl Payload for FilterLoad {
	fn version() -> u32 {
		70001
	}

	fn command() -> &'static str {
		"filterload"
	}

	fn deserialize_payload<T>(reader: &mut Reader<T>, _version: u32) -> MessageResult<Self> where T: io::Read {
		let filterload = FilterLoad {
			filter: try!(reader.read()),
			hash_functions: try!(reader.read()),
			tweak: try!(reader.read()),
			flags: try!(reader.read()),
		};

		Ok(filterload)
	}

	fn serialize_payload(&self, stream: &mut Stream, _version: u32) -> MessageResult<()> {
		stream
			.append(&self.filter)
			.append(&self.hash_functions)
			.append(&self.tweak)
			.append(&self.flags);
		Ok(())
	}
}

impl FilterFlags {
	pub fn from_u8(v: u8) -> Option<Self> {
		match v {
			0 => Some(FilterFlags::None),
			1 => Some(FilterFlags::All),
			2 => Some(FilterFlags::PubKeyOnly),
			_ => None,
		}
	}
}

impl From<FilterFlags> for u8 {
	fn from(i: FilterFlags) -> Self {
		i as u8
	}
}

impl Serializable for FilterFlags {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u8::from(*self));
	}
}

impl Deserializable for FilterFlags {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let t: u8 = try!(reader.read());
		FilterFlags::from_u8(t).ok_or(ReaderError::MalformedData)
	}
}
