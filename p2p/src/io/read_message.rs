use std::io;
use std::marker::PhantomData;
use futures::{Poll, Future, Async};
use message::{MessageResult, Error, Magic, Payload};
use io::{read_header, ReadHeader, read_payload, ReadPayload};

pub fn read_message<M, A>(a: A, magic: Magic, version: u32) -> ReadMessage<M, A>
	where A: io::Read, M: Payload {
	ReadMessage {
		state: ReadMessageState::ReadHeader {
			version: version,
			future: read_header(a, magic),
		},
		message_type: PhantomData
	}
}

enum ReadMessageState<M, A> {
	ReadHeader {
		version: u32,
		future: ReadHeader<A>,
	},
	ReadPayload {
		future: ReadPayload<M, A>,
	},
	Finished,
}

pub struct ReadMessage<M, A> {
	state: ReadMessageState<M, A>,
	message_type: PhantomData<M>,
}

impl<M, A> Future for ReadMessage<M, A> where A: io::Read, M: Payload {
	type Item = (A, MessageResult<M>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			ReadMessageState::ReadHeader { version, ref mut future } => {
				let (read, header) = try_ready!(future.poll());
				let header = match header {
					Ok(header) => header,
					Err(err) => return Ok((read, Err(err)).into()),
				};
				if header.command != M::command() {
					return Ok((read, Err(Error::InvalidCommand)).into());
				}
				let future = read_payload(
					read, version, header.len as usize, header.checksum,
				);
				let next = ReadMessageState::ReadPayload {
					future: future,
				};
				(next, Async::NotReady)
			},
			ReadMessageState::ReadPayload { ref mut future } => {
				let (read, payload) = try_ready!(future.poll());
				(ReadMessageState::Finished, Async::Ready((read, payload)))
			},
			ReadMessageState::Finished => panic!("poll ReadMessage after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}
