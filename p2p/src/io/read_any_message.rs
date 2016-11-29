use std::io;
use futures::{Future, Poll, Async};
use tokio_core::io::{read_exact, ReadExact};
use crypto::checksum;
use network::Magic;
use message::{Error, MessageHeader, MessageResult, Command};
use bytes::Bytes;
use io::{read_header, ReadHeader};

pub fn read_any_message<A>(a: A, magic: Magic) -> ReadAnyMessage<A> where A: io::Read {
	ReadAnyMessage {
		state: ReadAnyMessageState::ReadHeader(read_header(a, magic)),
	}
}

pub enum ReadAnyMessageState<A> {
	ReadHeader(ReadHeader<A>),
	ReadPayload {
		header: MessageHeader,
		future: ReadExact<A, Bytes>
	},
	Finished,
}

pub struct ReadAnyMessage<A> {
	state: ReadAnyMessageState<A>,
}

impl<A> Future for ReadAnyMessage<A> where A: io::Read {
	type Item = MessageResult<(Command, Bytes)>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			ReadAnyMessageState::ReadHeader(ref mut header) => {
				let (stream, header) = try_ready!(header.poll());
				let header = match header {
					Ok(header) => header,
					Err(err) => return Ok(Err(err).into()),
				};
				let future = read_exact(stream, Bytes::new_with_len(header.len as usize));
				let next = ReadAnyMessageState::ReadPayload {
					header: header,
					future: future,
				};
				(next, Async::NotReady)
			},
			ReadAnyMessageState::ReadPayload { ref mut header, ref mut future } => {
				let (_stream, bytes) = try_ready!(future.poll());
				if checksum(&bytes) != header.checksum {
					return Ok(Err(Error::InvalidChecksum).into());
				}
				let next = ReadAnyMessageState::Finished;
				(next, Ok((header.command.clone(), bytes)).into())
			},
			ReadAnyMessageState::Finished => panic!("poll ReadAnyMessage after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use network::Magic;
	use message::Error;
	use super::read_any_message;

	#[test]
	fn test_read_any_message() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c00c765845303b6da97786".into();
		let name = "ping".into();
		let nonce = "5845303b6da97786".into();
		let expected = (name, nonce);

		assert_eq!(read_any_message(raw.as_ref(), Magic::Mainnet).wait().unwrap(), Ok(expected));
		assert_eq!(read_any_message(raw.as_ref(), Magic::Testnet).wait().unwrap(), Err(Error::InvalidMagic));
	}

	#[test]
	fn test_read_too_short_any_message() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c00c765845303b6da977".into();
		assert!(read_any_message(raw.as_ref(), Magic::Mainnet).wait().is_err());
	}


	#[test]
	fn test_read_any_message_with_invalid_checksum() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c01c765845303b6da97786".into();
		assert_eq!(read_any_message(raw.as_ref(), Magic::Mainnet).wait().unwrap(), Err(Error::InvalidChecksum));
	}
}
