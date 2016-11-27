use std::{fmt, error};
use ser::Error as ReaderError;

pub type MessageResult<T> = Result<T, Error>;

#[derive(Debug, PartialEq, Clone)]
pub enum Error {
	/// Deserialization failed.
	Deserialize,
	/// Command has wrong format or is unsupported.
	InvalidCommand,
	/// Network magic comes from different network.
	InvalidMagic,
	/// Invalid checksum.
	InvalidChecksum,
	/// Invalid version.
	InvalidVersion,
}

impl From<ReaderError> for Error {
	fn from(_: ReaderError) -> Self {
		Error::Deserialize
	}
}

impl fmt::Display for Error {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(error::Error::description(self))
	}
}

impl error::Error for Error {
	fn description(&self) -> &str {
		match *self {
			Error::Deserialize => "Message Deserialization Error",
			Error::InvalidCommand => "Invalid Message Command",
			Error::InvalidMagic => "Invalid Network Magic",
			Error::InvalidChecksum => "Invalid message chacksum",
			Error::InvalidVersion => "Unsupported protocol version",
		}
	}
}
