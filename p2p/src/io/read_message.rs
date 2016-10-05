use std::io::{self, Read};
use futures::{Future, Poll, Async};
use futures::stream::Stream;
use message::{Message, MessageHeader};
use message::common::Magic;
use io::{read_header, read_payload, ReadHeader, ReadPayload, ReadRc};
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

pub fn read_message_stream<A>(a: A, magic: Magic, version: u32) -> ReadMessageStream<A> where A: io::Read {
	let stream: ReadRc<A> = a.into();
	ReadMessageStream {
		future: read_message(stream.clone(), magic, version),
		magic: magic,
		version: version,
		stream: stream,
	}
}

pub struct ReadMessage<A> {
	state: ReadMessageState<A>,
}

pub struct ReadMessageStream<A> {
	future: ReadMessage<ReadRc<A>>,
	magic: Magic,
	version: u32,
	stream: ReadRc<A>,
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

impl<A> Stream for ReadMessageStream<A> where A: io::Read {
	type Item = Message;
	type Error = Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		let result = match self.future.poll() {
			Ok(Async::Ready((_, result))) => Ok(Some(result).into()),
			Ok(Async::NotReady) => return Ok(Async::NotReady),
			Err(Error::Io(err)) => return Err(Error::Io(err)),
			Err(err) => {
				try!(self.stream.read_to_end(&mut Vec::new()));
				Err(err)
			}
		};

		self.future = read_message(self.stream.clone(), self.magic, self.version);
		result
	}
}

#[cfg(test)]
mod tests {
	use std::io::Cursor;
	use futures::Future;
	use futures::stream::Stream;
	use bytes::Bytes;
	use message::{Message, Payload};
	use message::common::Magic;
	use super::{read_message, read_message_stream};

	#[test]
	fn test_read_message() {
		let raw: Bytes = "f9beb4d976657261636b000000000000000000005df6e0e2".into();
		let expected = Message::new(Magic::Mainnet, Payload::Verack);
		assert_eq!(read_message(raw.as_ref(), Magic::Mainnet, 0).wait().unwrap().1, expected);
	}

	#[test]
	fn test_read_message_stream() {
		let raw: Bytes = "f9beb4d976657261636b000000000000000000005df6e0e2f9beb4d9676574616464720000000000000000005df6e0e2".into();
		let expected0 = Message::new(Magic::Mainnet, Payload::Verack);
		let expected1 = Message::new(Magic::Mainnet, Payload::GetAddr);

		let mut stream = read_message_stream(Cursor::new(raw), Magic::Mainnet, 0);
		assert_eq!(stream.poll().unwrap(), Some(expected0).into());
		assert_eq!(stream.poll().unwrap(), Some(expected1).into());
		assert!(stream.poll().is_err());
	}
}
