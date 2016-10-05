use ser::Error as ReaderError;

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
}

impl From<ReaderError> for Error {
	fn from(_: ReaderError) -> Self {
		Error::Deserialize
	}
}
