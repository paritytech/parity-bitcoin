use std::{net, io};
use tokio_core::io::{Io, ReadHalf, WriteHalf};
use message::{Message, Payload};
use message::common::{Magic, InventoryVector};
use message::types::{Ping, Pong, addr, Inv, GetData, NotFound};
use io::{write_message, WriteMessage, read_message_stream, ReadMessageStream};

pub struct Connection<A> {
	pub stream: A,
	pub version: u32,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

pub struct ConnectionReader<A> {
	stream: A,
	version: u32,
	magic: Magic,
}

pub struct ConnectionWriter<A> {
	stream: A,
	version: u32,
	magic: Magic,
}

impl<A> Connection<A> where A: Io {
	/// This function will panic if a task is not currently running.
	pub fn split(self) -> (ConnectionReader<ReadHalf<A>>, ConnectionWriter<WriteHalf<A>>) {
		let (r, w) = self.stream.split();
		let reader = ConnectionReader {
			stream: r,
			version: self.version,
			magic: self.magic,
		};
		let writer = ConnectionWriter {
			stream: w,
			version: self.version,
			magic: self.magic,
		};
		(reader, writer)
	}
}

impl<A> io::Read for ConnectionReader<A> where A: io::Read {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		self.stream.read(buf)
	}
}

impl<A> io::Write for ConnectionWriter<A> where A: io::Write {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
		self.stream.write(buf)
	}

	fn flush(&mut self) -> Result<(), io::Error> {
		self.stream.flush()
	}
}

impl<A> ConnectionReader<A> where A: io::Read {
	pub fn incoming(self) -> ReadMessageStream<A> {
		read_message_stream(self.stream, self.magic, self.version)
	}
}

impl<A> ConnectionWriter<A> where A: io::Write {
	fn write_message(self, payload: Payload) -> WriteMessage<ConnectionWriter<A>> {
		let message = Message::new(self.magic, payload);
		write_message(self, message)
	}

	pub fn ping(self) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::Ping(Ping {
			nonce: 0,
		});
		self.write_message(payload)
	}

	pub fn pong(self, nonce: u64) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::Pong(Pong {
			nonce: nonce,
		});
		self.write_message(payload)
	}

	pub fn getaddr(self) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::GetAddr;
		self.write_message(payload)
	}

	pub fn addr(self, addresses: Vec<addr::AddressEntry>) -> WriteMessage<ConnectionWriter<A>> {
		let payload = if self.version < 31402 {
			Payload::AddrBelow31402(addr::AddrBelow31402 {
				addresses: addresses.into_iter().map(|x| x.address).collect(),
			})
		} else {
			Payload::Addr(addr::Addr {
				addresses: addresses,
			})
		};
		self.write_message(payload)
	}

	pub fn inv(self, inventory: Vec<InventoryVector>) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::Inv(Inv {
			inventory: inventory,
		});
		self.write_message(payload)
	}

	pub fn getdata(self, inventory: Vec<InventoryVector>) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::GetData(GetData {
			inventory: inventory,
		});
		self.write_message(payload)
	}

	pub fn notfound(self, inventory: Vec<InventoryVector>) -> WriteMessage<ConnectionWriter<A>> {
		let payload = Payload::NotFound(NotFound {
			inventory: inventory,
		});
		self.write_message(payload)
	}
}

#[cfg(test)]
mod test {
	use std::io::Cursor;
	use futures::stream::Stream;
	use bytes::Bytes;
	use message::{Message, Payload};
	use message::common::Magic;
	use super::ConnectionReader;

	#[test]
	fn test_connection_reader_stream() {
		let raw: Bytes = "f9beb4d976657261636b000000000000000000005df6e0e2f9beb4d9676574616464720000000000000000005df6e0e2".into();
		let expected0 = Message::new(Magic::Mainnet, Payload::Verack);
		let expected1 = Message::new(Magic::Mainnet, Payload::GetAddr);

		let reader = ConnectionReader {
			stream: Cursor::new(raw),
			version: 0,
			magic: Magic::Mainnet,
		};

		let mut incoming = reader.incoming();
		assert_eq!(incoming.poll().unwrap(), Some(expected0).into());
		assert_eq!(incoming.poll().unwrap(), Some(expected1).into());
	}
}
