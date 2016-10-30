use std::{io, cmp};
use futures::{Future, Poll, Async};
use message::{Message, MessageResult};
use message::types::{Version, Verack};
use message::common::Magic;
use io::{write_message, WriteMessage, ReadMessage, read_message};

pub fn handshake<A>(a: A, magic: Magic, version: Version) -> Handshake<A> where A: io::Write + io::Read {
	Handshake {
		version: version.version(),
		state: HandshakeState::SendVersion(write_message(a, version_message(magic, version))),
		magic: magic,
	}
}

pub fn accept_handshake<A>(a: A, magic: Magic, version: Version) -> AcceptHandshake<A> where A: io::Write + io::Read {
	AcceptHandshake {
		version: version.version(),
		state: AcceptHandshakeState::ReceiveVersion {
			local_version: Some(version),
			future: read_message(a, magic, 0),
		},
		magic: magic,
	}
}

pub fn negotiate_version(local: u32, other: u32) -> u32 {
	cmp::min(local, other)
}

#[derive(Debug, PartialEq)]
pub struct HandshakeResult {
	pub version: Version,
	pub negotiated_version: u32,
}

fn version_message(magic: Magic, version: Version) -> Message<Version> {
	Message::new(magic, version.version(), &version).expect("version message should always be serialized correctly")
}

fn verack_message(magic: Magic) -> Message<Verack> {
	Message::new(magic, 0, &Verack).expect("verack message should always be serialized correctly")
}

enum HandshakeState<A> {
	SendVersion(WriteMessage<Version, A>),
	ReceiveVersion(ReadMessage<Version, A>),
	ReceiveVerack {
		version: Option<Version>,
		future: ReadMessage<Verack, A>,
	},
	Finished,
}

enum AcceptHandshakeState<A> {
	ReceiveVersion {
		local_version: Option<Version>,
		future: ReadMessage<Version, A>
	},
	SendVersion {
		version: Option<Version>,
		future: WriteMessage<Version, A>,
	},
	SendVerack {
		version: Option<Version>,
		future: WriteMessage<Verack, A>,
	},
	Finished,
}

pub struct Handshake<A> {
	state: HandshakeState<A>,
	magic: Magic,
	version: u32,
}

pub struct AcceptHandshake<A> {
	state: AcceptHandshakeState<A>,
	magic: Magic,
	version: u32,
}

impl<A> Future for Handshake<A> where A: io::Read + io::Write {
	type Item = (A, MessageResult<HandshakeResult>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			HandshakeState::SendVersion(ref mut future) => {
				let (stream, _) = try_ready!(future.poll());
				(HandshakeState::ReceiveVersion(read_message(stream, self.magic, 0)), Async::NotReady)
			},
			HandshakeState::ReceiveVersion(ref mut future) => {
				let (stream, version) = try_ready!(future.poll());
				let version = match version {
					Ok(version) => version,
					Err(err) => return Ok((stream, Err(err.into())).into()),
				};

				let next = HandshakeState::ReceiveVerack {
					version: Some(version),
					future: read_message(stream, self.magic, 0),
				};

				(next, Async::NotReady)
			},
			HandshakeState::ReceiveVerack { ref mut version, ref mut future } => {
				let (stream, _verack) = try_ready!(future.poll());
				let version = version.take().expect("verack must be preceded by version");

				let result = HandshakeResult {
					negotiated_version: negotiate_version(self.version, version.version()),
					version: version,
				};

				(HandshakeState::Finished, Async::Ready((stream, Ok(result))))
			},
			HandshakeState::Finished => panic!("poll Handshake after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}

impl<A> Future for AcceptHandshake<A> where A: io::Read + io::Write {
	type Item = (A, MessageResult<HandshakeResult>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			AcceptHandshakeState::ReceiveVersion { ref mut local_version, ref mut future } => {
				let (stream, version) = try_ready!(future.poll());
				let version = match version {
					Ok(version) => version,
					Err(err) => return Ok((stream, Err(err.into())).into()),
				};

				let local_version = local_version.take().expect("local version must be set");
				let next = AcceptHandshakeState::SendVersion {
					version: Some(version),
					future: write_message(stream, version_message(self.magic, local_version)),
				};

				(next, Async::NotReady)
			},
			AcceptHandshakeState::SendVersion { ref mut version, ref mut future } => {
				let (stream, _) = try_ready!(future.poll());
				let next = AcceptHandshakeState::SendVerack {
					version: version.take(),
					future: write_message(stream, verack_message(self.magic)),
				};

				(next, Async::NotReady)
			},
			AcceptHandshakeState::SendVerack { ref mut version, ref mut future } => {
				let (stream, _) = try_ready!(future.poll());

				let version = version.take().expect("verack must be preceded by version");

				let result = HandshakeResult {
					negotiated_version: negotiate_version(self.version, version.version()),
					version: version,
				};

				(AcceptHandshakeState::Finished, Async::Ready((stream, Ok(result))))
			},
			AcceptHandshakeState::Finished => panic!("poll AcceptHandshake after it's done"),
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
	use std::io;
	use futures::Future;
	use bytes::Bytes;
	use ser::Stream;
	use message::{Magic, Message};
	use message::types::Verack;
	use message::types::version::{Version, V0, V106, V70001};
	use super::{handshake, accept_handshake, HandshakeResult};

	pub struct TestIo {
		read: io::Cursor<Bytes>,
		write: Bytes,
	}

	impl io::Read for TestIo {
		fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
			io::Read::read(&mut self.read, buf)
		}
	}

	impl io::Write for TestIo {
		fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
			io::Write::write(&mut self.write, buf)
		}

		fn flush(&mut self) -> io::Result<()> {
			io::Write::flush(&mut self.write)
		}
	}

	fn local_version() -> Version {
		Version::V70001(V0 {
			version: 70001,
			services: 1u64.into(),
			timestamp: 0x4d1015e6,
			// address and port of remote
			// services set to 0, cause we know nothing about the node
			receiver: "00000000000000000000000000000000000000002f5a0808208d".into(),
		}, V106 {
			// our local address (not sure if it is valid, or if it is checked at all
			// services set to 0, because we support nothing
			from: "00000000000000000000000000000000000000007f000001208d".into(),
			nonce: 0x3c76a409eb48a227,
			user_agent: "pbtc".into(),
			start_height: 0,
		}, V70001 {
			relay: true,
		})
	}

	fn remote_version() -> Version {
		Version::V70001(V0 {
			version: 70012,
			services: 1u64.into(),
			timestamp: 0x4d1015e6,
			// services set to 1, house receiver supports at least the network
			receiver: "010000000000000000000000000000000000ffffc2b5936adde9".into(),
		}, V106 {
			// remote address, port
			// and supported protocols
			from: "050000000000000000000000000000000000ffff2f5a0808208d".into(),
			nonce: 0x3c76a409eb48a227,
			user_agent: "/Satoshi:0.12.1/".into(),
			start_height: 0,
		}, V70001 {
			relay: true,
		})
	}

	#[test]
	fn test_handshake() {
		let magic = Magic::Mainnet;
		let version = 70012;
		let local_version = local_version();
		let remote_version = remote_version();

		let mut remote_stream = Stream::new();
		remote_stream.append_slice(Message::new(magic, version, &remote_version).unwrap().as_ref());
		remote_stream.append_slice(Message::new(magic, version, &Verack).unwrap().as_ref());

		let expected = HandshakeResult {
			version: remote_version,
			negotiated_version: 70001,
		};

		let mut expected_stream = Stream::new();
		expected_stream.append_slice(Message::new(magic, version, &local_version).unwrap().as_ref());

		let test_io = TestIo {
			read: io::Cursor::new(remote_stream.out()),
			write: Bytes::default(),
		};

		let hs = handshake(test_io, magic, local_version).wait().unwrap();
		assert_eq!(hs.0.write, expected_stream.out());
		assert_eq!(hs.1.unwrap(), expected);
	}

	#[test]
	fn test_accept_handshake() {
		let magic = Magic::Mainnet;
		let version = 70012;
		let local_version = local_version();
		let remote_version = remote_version();

		let mut remote_stream = Stream::new();
		remote_stream.append_slice(Message::new(magic, version, &remote_version).unwrap().as_ref());

		let test_io = TestIo {
			read: io::Cursor::new(remote_stream.out()),
			write: Bytes::default(),
		};

		let expected = HandshakeResult {
			version: remote_version,
			negotiated_version: 70001,
		};

		let mut expected_stream = Stream::new();
		expected_stream.append_slice(Message::new(magic, version, &local_version).unwrap().as_ref());
		expected_stream.append_slice(Message::new(magic, version, &Verack).unwrap().as_ref());

		let hs = accept_handshake(test_io, magic, local_version).wait().unwrap();
		assert_eq!(hs.0.write, expected_stream.out());
		assert_eq!(hs.1.unwrap(), expected);
	}
}
