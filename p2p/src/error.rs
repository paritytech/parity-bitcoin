use std::io;
use message::Error as MessageError;

#[derive(Debug)]
pub enum Error {
	Io(io::Error),
	Message(MessageError),
	Handshake,
}

impl From<io::Error> for Error {
	fn from(e: io::Error) -> Self {
		Error::Io(e)
	}
}

impl From<MessageError> for Error {
	fn from(e: MessageError) -> Self {
		Error::Message(e)
	}
}
