use std::io;
use ser::{Error as ReaderError};

#[derive(Debug)]
pub enum Error {
	Io(io::Error),
	Data(ReaderError),
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

