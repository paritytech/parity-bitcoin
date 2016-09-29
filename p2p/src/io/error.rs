use std::io;
use ser::{Error as ReaderError};

#[derive(Debug)]
pub enum Error {
	Io(io::Error),
	Data(ReaderError),
	InvalidNetwork,
	InvalidChecksum,
	HandshakeFailed,
}

impl Error {
	pub fn kind(&self) -> io::ErrorKind {
		match *self {
			Error::Io(ref e) => e.kind(),
			Error::Data(_) |
			Error::HandshakeFailed |
			Error::InvalidChecksum |
			Error::InvalidNetwork => io::ErrorKind::Other,
		}
	}
}

impl From<io::Error> for Error {
	fn from(e: io::Error) -> Self {
		Error::Io(e)
	}
}

impl From<ReaderError> for Error {
	fn from(e: ReaderError) -> Self {
		Error::Data(e)
	}
}

