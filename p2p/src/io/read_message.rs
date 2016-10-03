use std::io;
use futures::{Future, Poll, Async};
use message::{Message, MessageHeader};
use message::common::Magic;
use io::{read_header, read_payload, ReadHeader, ReadPayload};
use Error;

enum ReadMessageState<A> {
	ReadHeader {
		version: u32,
		future: ReadHeader<A>,
	},
	ReadPayload {
		header: Option<MessageHeader>,
		future: ReadPayload<A>
	},
	Finished,
}

pub fn read_message<A>(a: A, magic: Magic, version: u32) -> ReadMessage<A> where A: io::Read {
	ReadMessage {
		state: ReadMessageState::ReadHeader {
			version: version,
			future: read_header(a, magic)
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
		let (next, result) = match self.state {
			ReadMessageState::ReadHeader { version, ref mut future } => {
				let (read, header) = try_ready!(future.poll());
				let future = read_payload(
					read, version, header.len as usize,
					header.command.clone(), header.checksum.clone()
				);
				let next = ReadMessageState::ReadPayload {
					future: future,
					header: Some(header),
				};
				(next, Async::NotReady)
			},
			ReadMessageState::ReadPayload { ref mut header, ref mut future } => {
				let (read, payload) = try_ready!(future.poll());
				let message = Message {
					header: header.take().expect("payload must be preceded by header"),
					payload: payload,
				};

				(ReadMessageState::Finished, Async::Ready((read, message)))
			},
			ReadMessageState::Finished => panic!("poll AcceptHandshake after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}

#[cfg(test)]
mod tests {
	use futures::Future;
	use bytes::Bytes;
	use message::{Message, Payload};
	use message::common::Magic;
	use super::read_message;

	#[test]
	fn test_read_message() {
		let raw: Bytes = "f9beb4d976657261636b000000000000000000005df6e0e2".into();
		let expected = Message::new(Magic::Mainnet, Payload::Verack);
		assert_eq!(read_message(raw.as_ref(), Magic::Mainnet, 0).wait().unwrap().1, expected);
	}
}
