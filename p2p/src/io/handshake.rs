use std::io;
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
}

enum HandshakeState<A> {
	SendVersion(WriteMessage<A>),
	ReceiveVersion(ReadMessage<A>),
	ReceiveVerack {
		version: Option<Version>,
		future: ReadMessage<A>,
	},
	Finished,
}

enum AcceptHandshakeState<A> {
	ReceiveVersion(ReadMessage<A>),
	SendVersion {
		version: Option<Version>,
		future: WriteMessage<A>,
	},
	SendVerack {
		version: Option<Version>,
		future: WriteMessage<A>,
	},
	Finished,
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
		let (next, result) = match self.state {
			HandshakeState::SendVersion(ref mut future) => {
				let (stream, _) = try_async!(future.poll());
				(HandshakeState::ReceiveVersion(read_message(stream, 0)), Async::NotReady)
			},
			HandshakeState::ReceiveVersion(ref mut future) => {
				let (stream, message) = try_async!(future.poll());
				let version = match message.payload {
					Payload::Version(version) => version,
					_ => return Err(Error::HandshakeFailed),
				};

				let next = HandshakeState::ReceiveVerack {
					version: Some(version),
					future: read_message(stream, 0),
				};

				(next, Async::NotReady)
			},
			HandshakeState::ReceiveVerack { ref mut version, ref mut future } => {
				let (stream, message) = try_async!(future.poll());
				if message.payload != Payload::Verack {
					return Err(Error::HandshakeFailed);
				}

				let result = HandshakeResult {
					version: version.take().expect("verack must be preceded by version"),
				};

				(HandshakeState::Finished, Async::Ready((stream, result)))
			},
			HandshakeState::Finished => panic!("poll Handshake after it's done"),
		};

		self.state = next;
		Ok(result)
	}
}

impl<A> Future for AcceptHandshake<A> where A: io::Read + io::Write {
	type Item = (A, HandshakeResult);
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			AcceptHandshakeState::ReceiveVersion(ref mut future) => {
				let (stream, message) = try_async!(future.poll());
				let version = match message.payload {
					Payload::Version(version) => version,
					_ => return Err(Error::HandshakeFailed),
				};

				let next = AcceptHandshakeState::SendVersion {
					version: Some(version),
					future: write_message(stream, &local_version()),
				};

				(next, Async::NotReady)
			},
			AcceptHandshakeState::SendVersion { ref mut version, ref mut future } => {
				let (stream, _) = try_async!(future.poll());
				let next = AcceptHandshakeState::SendVerack {
					version: version.take(),
					future: write_message(stream, &verack()),
				};

				(next, Async::NotReady)
			},
			AcceptHandshakeState::SendVerack { ref mut version, ref mut future } => {
				let (stream, _) = try_async!(future.poll());

				let result = HandshakeResult {
					version: version.take().expect("verack must be preceded by version"),
				};

				(AcceptHandshakeState::Finished, Async::Ready((stream, result)))
			},
			AcceptHandshakeState::Finished => panic!("poll AcceptHandshake after it's done"),
		};

		self.state = next;
		Ok(result)
	}
}
