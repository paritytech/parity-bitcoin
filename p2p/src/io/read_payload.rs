use std::io;
use futures::{Future, Poll, Async};
use tokio_core::io::{ReadExact, read_exact};
use bytes::Bytes;
use hash::H32;
use message::{Payload, MessageResult};
use message::common::Command;

pub fn read_payload<A>(a: A, version: u32, len: usize, command: Command, checksum: H32) -> ReadPayload<A> where A: io::Read {
	ReadPayload {
		reader: read_exact(a, Bytes::new_with_len(len)),
		version: version,
		command: command,
		checksum: checksum,
	}
}

pub struct ReadPayload<A> {
	reader: ReadExact<A, Bytes>,
	version: u32,
	command: Command,
	checksum: H32,
}

impl<A> Future for ReadPayload<A> where A: io::Read {
	type Item = (A, MessageResult<Payload>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (read, data) = try_ready!(self.reader.poll());
		let payload = Payload::deserialize(&data, &self.checksum, self.version, &self.command);
		Ok(Async::Ready((read, payload)))
	}
}
