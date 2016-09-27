use std::io;
use futures::{Future, Poll, Async};
use net::messages::{Message, MessageHeader};
use io::{read_header, read_payload, ReadHeader, ReadPayload, Error};

enum ReadMessageState<A> {
	ReadHeader {
		version: u32,
		future: ReadHeader<A>,
	},
	ReadPayload {
		header: MessageHeader,
		future: ReadPayload<A>
	},
}

pub fn read_message<A>(a: A, version: u32) -> ReadMessage<A> where A: io::Read {
	ReadMessage {
		state: ReadMessageState::ReadHeader {
			version: version,
			future: read_header(a)
		},
	}
}

pub struct ReadMessage<A> {
	state: ReadMessageState<A>,
}

impl<A> Future for ReadMessage<A> where A: io::Read {
	type Item = (A, Message);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let next = match self.state {
			ReadMessageState::ReadHeader { version, ref mut future } => {
				let (read, header) = try_async!(future.poll());
				ReadMessageState::ReadPayload {
					future: read_payload(read, version, header.len as usize, header.command.clone()),
					header: header,
				}
			},
			ReadMessageState::ReadPayload { ref header, ref mut future } => {
				let (read, payload) = try_async!(future.poll());
				let message = Message {
					header: header.clone(),
					payload: payload,
				};

				return Ok(Async::Ready((read, message)))
			},
		};

		self.state = next;
		Ok(Async::NotReady)
	}
}

