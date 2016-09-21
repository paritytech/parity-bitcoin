use std::io;
use futures::{Future, Poll, Async};
use tokio_core::io::{ReadExact, read_exact};
use ser::deserialize;
use net::messages::MessageHeader;
use io::Error;

pub fn read_header<A>(a: A) -> ReadHeader<A> where A: io::Read {
	ReadHeader {
		reader: read_exact(a, [0u8; 24]),
	}
}

pub struct ReadHeader<A> {
	reader: ReadExact<A, [u8; 24]>,
}

impl<A> Future for ReadHeader<A> where A: io::Read {
	type Item = (A, MessageHeader);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		match try_nb!(self.reader.poll()) {
			Async::Ready((read, data)) => {
				let header: MessageHeader = try!(deserialize(&data));
				Ok(Async::Ready((read, header)))
			},
			_ => Ok(Async::NotReady),
		}
	}
}
