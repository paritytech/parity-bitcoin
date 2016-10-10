use std::io;
use std::marker::PhantomData;
use futures::{Poll, Future, Async};
use ser::Deserializable;
use message::{MessageResult, Error};
use message::common::Magic;
use message::serialization::PayloadType;
use io::{read_header, ReadHeader, read_specific_payload, ReadSpecificPayload};

pub fn read_specific_message<M, A>(a: A, magic: Magic, version: u32) -> ReadSpecificMessage<M, A>
	where A: io::Read, M: PayloadType + Deserializable {
	ReadSpecificMessage {
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
		future: ReadSpecificPayload<M, A>,
	},
	Finished,
}

pub struct ReadSpecificMessage<M, A> {
	state: ReadMessageState<M, A>,
	message_type: PhantomData<M>,
}

impl<M, A> Future for ReadSpecificMessage<M, A> where A: io::Read, M: PayloadType + Deserializable {
	type Item = (A, MessageResult<M>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			ReadMessageState::ReadHeader { version, ref mut future } => {
				let (read, header) = try_ready!(future.poll());
				let header = match header {
					Ok(header) => header,
					Err(err) => {
						return Ok((read, Err(err)).into());
					}
				};
				if header.command != M::command().into() {
					return Ok((read, Err(Error::InvalidCommand)).into());
				}
				let future = read_specific_payload(
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
			ReadMessageState::Finished => panic!("poll ReadSpecificMessage after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}
