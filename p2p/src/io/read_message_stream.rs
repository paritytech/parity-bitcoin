use std::io;
use futures::{Future, Poll, Async};
use futures::stream::Stream;
use tokio_core::io::{read_exact, ReadExact};
use crypto::checksum;
use message::{Error, MessageHeader, MessageResult, Magic, Command};
use bytes::Bytes;
use io::{read_header, ReadHeader};

pub fn read_message_stream<A>(a: A, magic: Magic) -> ReadMessageStream<A> where A: io::Read {
	ReadMessageStream {
		state: ReadMessageStreamState::ReadHeader(read_header(a, magic)),
		magic: magic,
	}
}

pub enum ReadMessageStreamState<A> {
	ReadHeader(ReadHeader<A>),
	ReadPayload {
		header: MessageHeader,
		future: ReadExact<A, Bytes>
	},
}

pub struct ReadMessageStream<A> {
	state: ReadMessageStreamState<A>,
	magic: Magic,
}

impl<A> Stream for ReadMessageStream<A> where A: io::Read {
	type Item = MessageResult<(Command, Bytes)>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		let (next, result) = match self.state {
			ReadMessageStreamState::ReadHeader(ref mut header) => {
				let (stream, header) = try_ready!(header.poll());
				let header = match header {
					Ok(header) => header,
					Err(err) => return Ok(Some(Err(err)).into()),
				};
				let future = read_exact(stream, Bytes::new_with_len(header.len as usize));
				let next = ReadMessageStreamState::ReadPayload {
					header: header,
					future: future,
				};
				(next, Async::NotReady)
			},
			ReadMessageStreamState::ReadPayload { ref mut header, ref mut future } => {
				let (stream, bytes) = try_ready!(future.poll());
				if checksum(&bytes) != header.checksum {
					return Ok(Some(Err(Error::InvalidChecksum)).into());
				}
				let future = read_header(stream, self.magic);
				let next = ReadMessageStreamState::ReadHeader(future);
				(next, Some(Ok((header.command.clone(), bytes))).into())
			},
		};

		self.state = next;
		Ok(result)
	}
}
