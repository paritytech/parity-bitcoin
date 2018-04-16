use std::io;
use futures::{Future, Poll, Async};
use tokio_io::AsyncRead;
use tokio_io::io::{ReadExact, read_exact};
use message::{MessageHeader, MessageResult};
use network::Magic;

pub fn read_header<A>(a: A, magic: Magic) -> ReadHeader<A> where A: AsyncRead {
	ReadHeader {
		reader: read_exact(a, [0u8; 24]),
		magic: magic,
	}
}

pub struct ReadHeader<A> {
	reader: ReadExact<A, [u8; 24]>,
	magic: Magic,
}

impl<A> Future for ReadHeader<A> where A: AsyncRead {
	type Item = (A, MessageResult<MessageHeader>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (read, data) = try_ready!(self.reader.poll());
		let header = MessageHeader::deserialize(&data, self.magic);
		Ok(Async::Ready((read, header)))
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use network::{Network, ConsensusFork};
	use message::{MessageHeader, Error};
	use super::read_header;

	#[test]
	fn test_read_header() {
		let raw: Bytes = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let expected = MessageHeader {
			magic: Network::Mainnet.magic(&ConsensusFork::BitcoinCore),
			command: "addr".into(),
			len: 0x1f,
			checksum: "ed52399b".into(),
		};

		assert_eq!(read_header(raw.as_ref(), Network::Mainnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap().1, Ok(expected));
		assert_eq!(read_header(raw.as_ref(), Network::Testnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap().1, Err(Error::InvalidMagic));
	}

	#[test]
	fn test_read_header_with_invalid_magic() {
		let raw: Bytes = "f9beb4d86164647200000000000000001f000000ed52399b".into();
		assert_eq!(read_header(raw.as_ref(), Network::Testnet.magic(&ConsensusFork::BitcoinCore)).wait().unwrap().1, Err(Error::InvalidMagic));
	}

	#[test]
	fn test_read_too_short_header() {
		let raw: Bytes = "f9beb4d96164647200000000000000001f000000ed5239".into();
		assert!(read_header(raw.as_ref(), Network::Mainnet.magic(&ConsensusFork::BitcoinCore)).wait().is_err());
	}
}
