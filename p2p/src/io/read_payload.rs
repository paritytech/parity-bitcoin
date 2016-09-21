use std::io;
use futures::{Future, Poll, Async};
use tokio_core::io::{ReadExact, read_exact};
use bytes::Bytes;
use net::common::Command;
use net::messages::{Payload, deserialize_payload};
use io::Error;

pub fn read_payload<A>(a: A, version: u32, len: usize, command: Command) -> ReadPayload<A> where A: io::Read {
	ReadPayload {
		reader: read_exact(a, Bytes::new_with_len(len)),
		version: version,
		command: command,
	}
}

pub struct ReadPayload<A> {
	reader: ReadExact<A, Bytes>,
	version: u32,
	command: Command,
}

impl<A> Future for ReadPayload<A> where A: io::Read {
	type Item = (A, Payload);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		match try_nb!(self.reader.poll()) {
			Async::Ready((read, data)) => {
				let payload = try!(deserialize_payload(&data, self.version, &self.command));
				Ok(Async::Ready((read, payload)))
			},
			Async::NotReady => Ok(Async::NotReady),
		}
	}
}
