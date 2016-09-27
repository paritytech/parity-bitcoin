use std::{io, cmp};
use futures::{Future, Poll, Async};
use net::messages::{Version, Message, Payload};
use io::{write_message, read_message, ReadMessage, WriteMessage, Error};

pub const VERSION: u32 = 106;

fn local_version() -> Message {
	unimplemented!();
}

pub struct HandshakeResult {
	pub version: Version,
	pub negotiated_version: u32,
}


enum HandshakeState<A> {
	SendVersion(WriteMessage<A>),
	ReceiveVersion(ReadMessage<A>),
	ReceiveVerack {
		version: Version,
		future: ReadMessage<A>,
	},
}

pub fn handshake<A>(a: A) -> Handshake<A> where A: io::Write + io::Read {
	Handshake {
		state: HandshakeState::SendVersion(write_message(a, &local_version())),
	}
}

pub struct Handshake<A> {
	state: HandshakeState<A>,
}

impl<A> Future for Handshake<A> where A: io::Read + io::Write {
	type Item = (A, HandshakeResult);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let next = match self.state {
			HandshakeState::SendVersion(ref mut future) => {
				let (stream, _) = try_async!(future.poll());
				HandshakeState::ReceiveVersion(read_message(stream, 0))
			},
			HandshakeState::ReceiveVersion(ref mut future) => {
				let (stream, message) = try_async!(future.poll());
				let version = match message.payload {
					Payload::Version(version) => version,
					_ => return Err(Error::HandshakeFailed),
				};

				HandshakeState::ReceiveVerack {
					version: version,
					future: read_message(stream, 0),
				}
			},
			HandshakeState::ReceiveVerack { ref version, ref mut future } => {
				let (stream, message) = try_async!(future.poll());
				if message.payload != Payload::Verack {
					return Err(Error::HandshakeFailed);
				}

				let result = HandshakeResult {
					version: version.clone(),
					negotiated_version: cmp::min(VERSION, version.version()),
				};

				return Ok(Async::Ready((stream, result)));
			}
		};

		self.state = next;
		Ok(Async::NotReady)
	}
}
