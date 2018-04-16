use std::io;
use futures::{Future, Poll, Async};
use tokio_io::io::{read_exact, ReadExact};
use tokio_io::AsyncRead;
use crypto::checksum;
use network::Magic;
use message::{Error, MessageHeader, MessageResult, Command};
use bytes::Bytes;
use io::{read_header, ReadHeader};

pub fn read_any_message<A>(a: A, magic: Magic) -> ReadAnyMessage<A> where A: AsyncRead {
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
}

pub struct ReadAnyMessage<A> {
	state: ReadAnyMessageState<A>,
}

impl<A> Future for ReadAnyMessage<A> where A: AsyncRead {
	type Item = MessageResult<(Command, Bytes)>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		loop {
			let next_state = match self.state {
				ReadAnyMessageState::ReadHeader(ref mut header) => {
					let (stream, header) = try_ready!(header.poll());
					let header = match header {
						Ok(header) => header,
						Err(err) => return Ok(Err(err).into()),
					};
					ReadAnyMessageState::ReadPayload {
						future: read_exact(stream, Bytes::new_with_len(header.len as usize)),
						header: header,
					}
				},
				ReadAnyMessageState::ReadPayload { ref mut header, ref mut future } => {
					let (_stream, bytes) = try_ready!(future.poll());
					if checksum(&bytes) != header.checksum {
						return Ok(Err(Error::InvalidChecksum).into());
					}

					return Ok(Async::Ready(Ok((header.command.clone(), bytes))));
				},
			};

			self.state = next_state;
		}
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use network::{Network, ConsensusFork};
	use message::Error;
	use super::read_any_message;

	#[test]
	fn test_read_any_message() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c00c765845303b6da97786".into();
		let name = "ping".into();
		let nonce = "5845303b6da97786".into();
		let expected = (name, nonce);

		assert_eq!(read_any_message(raw.as_ref(), Network::Mainnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap(), Ok(expected));
		assert_eq!(read_any_message(raw.as_ref(), Network::Testnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap(), Err(Error::InvalidMagic));
	}

	#[test]
	fn test_read_too_short_any_message() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c00c765845303b6da977".into();
		assert!(read_any_message(raw.as_ref(), Network::Mainnet.magic(&ConsensusFork::BitcoinCore)).wait().is_err());
	}


	#[test]
	fn test_read_any_message_with_invalid_checksum() {
		let raw: Bytes = "f9beb4d970696e6700000000000000000800000083c01c765845303b6da97786".into();
		assert_eq!(read_any_message(raw.as_ref(), Network::Mainnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap(), Err(Error::InvalidChecksum));
	}
}
