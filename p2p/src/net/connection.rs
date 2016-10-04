use std::{net, io};
use message::{Message, Payload};
use message::common::Magic;
use message::types::{Ping, Pong};
use io::{HandshakeResult, write_message, WriteMessage};

pub struct Connection<A> {
	pub stream: A,
	pub handshake_result: HandshakeResult,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

impl<A> io::Read for Connection<A> where A: io::Read {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		self.stream.read(buf)
	}
}

impl<A> io::Write for Connection<A> where A: io::Write {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
		self.stream.write(buf)
	}

	fn flush(&mut self) -> Result<(), io::Error> {
		self.stream.flush()
	}
}

impl<A> Connection<A> where A: io::Read + io::Write {
	pub fn ping(self) -> WriteMessage<Connection<A>> {
		let payload = Payload::Ping(Ping {
			nonce: 0,
		});

		let message = Message::new(self.magic, payload);
		write_message(self, message)
	}

	pub fn pong(self) -> WriteMessage<Connection<A>> {
		let payload = Payload::Pong(Pong {
			nonce: 0,
		});

		let message = Message::new(self.magic, payload);
		write_message(self, message)
	}
}
