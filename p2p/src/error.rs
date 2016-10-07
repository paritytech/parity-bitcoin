use message::Error as MessageError;

#[derive(Debug)]
pub enum Error {
	Message(MessageError),
	Handshake,
}

impl From<MessageError> for Error {
	fn from(e: MessageError) -> Self {
		Error::Message(e)
	}
}
