use std::io;
use futures::{Future, Poll, Async};
use tokio_core::io::{ReadExact, read_exact};
use ser::deserialize;
use net::messages::MessageHeader;
use net::common::Magic;
use io::Error;

pub fn read_header<A>(a: A, magic: Magic) -> ReadHeader<A> where A: io::Read {
	ReadHeader {
		reader: read_exact(a, [0u8; 24]),
		magic: magic,
	}
}

pub struct ReadHeader<A> {
	reader: ReadExact<A, [u8; 24]>,
	magic: Magic,
}

impl<A> Future for ReadHeader<A> where A: io::Read {
	type Item = (A, MessageHeader);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (read, data) = try_async!(self.reader.poll());
		let header: MessageHeader = try!(deserialize(&data));
		if header.magic != self.magic {
			return Err(Error::InvalidNetwork);
		}
		Ok(Async::Ready((read, header)))
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use net::messages::MessageHeader;
	use net::common::Magic;
	use super::read_header;

	#[test]
	fn test_read_header() {
		let raw: Bytes = "f9beb4d96164647200000000000000001f000000ed52399b".into();
		let expected = MessageHeader {
			magic: Magic::Mainnet,
			command: "addr".into(),
			len: 0x1f,
			checksum: [0xed, 0x52, 0x39, 0x9b],
		};

		assert_eq!(read_header(raw.as_ref(), Magic::Mainnet).wait().unwrap().1, expected);
		assert!(read_header(raw.as_ref(), Magic::Testnet).wait().is_err());
	}
}
