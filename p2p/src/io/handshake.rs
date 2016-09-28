use std::{io, cmp};
use futures::{Future, Poll, Async};
use net::messages::{Version, Message, Payload};
use io::{write_message, read_message, ReadMessage, WriteMessage, Error};

pub const VERSION: u32 = 106;

fn local_version() -> Message {
	unimplemented!();
}

fn verack() -> Message {
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

enum AcceptHandshakeState<A> {
	ReceiveVersion(ReadMessage<A>),
	SendVersion {
		version: Version,
		future: WriteMessage<A>,
	},
	SendVerack {
		version: Version,
		future: WriteMessage<A>,
	}
}

pub fn handshake<A>(a: A) -> Handshake<A> where A: io::Write + io::Read {
	Handshake {
		state: HandshakeState::SendVersion(write_message(a, &local_version())),
	}
}

pub fn accept_handshake<A>(a: A) -> AcceptHandshake<A> where A: io::Write + io::Read {
	AcceptHandshake {
		state: AcceptHandshakeState::ReceiveVersion(read_message(a, 0)),
	}
}

pub struct Handshake<A> {
	state: HandshakeState<A>,
}

pub struct AcceptHandshake<A> {
	state: AcceptHandshakeState<A>,
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

impl<A> Future for AcceptHandshake<A> where A: io::Read + io::Write {
	type Item = (A, HandshakeResult);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let next = match self.state {
			AcceptHandshakeState::ReceiveVersion(ref mut future) => {
				let (stream, message) = try_async!(future.poll());
				let version = match message.payload {
					Payload::Version(version) => version,
					_ => return Err(Error::HandshakeFailed),
				};
				AcceptHandshakeState::SendVersion {
					version: version,
					future: write_message(stream, &local_version()),
				}
			},
			AcceptHandshakeState::SendVersion { ref version, ref mut future } => {
				let (stream, _) = try_async!(future.poll());
				AcceptHandshakeState::SendVerack {
					version: version.clone(),
					future: write_message(stream, &verack()),
				}
			},
			AcceptHandshakeState::SendVerack { ref version, ref mut future } => {
				let (stream, _) = try_async!(future.poll());

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
