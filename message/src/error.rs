use ser::Error as ReaderError;

pub type MessageResult<T> = Result<T, Error>;

#[derive(Debug, PartialEq)]
pub enum Error {
	/// Deserialization failed.
	Deserialize,
	/// Command has wrong format or is unsupported.
	InvalidCommand,
	/// Network magic is not supported.
	InvalidMagic,
	/// Network magic comes from different network.
	WrongMagic,
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
