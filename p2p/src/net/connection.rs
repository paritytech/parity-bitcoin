use std::{net, io};
use message::{Message, Payload};
use message::common::{Magic, InventoryVector};
use message::types::{Ping, Pong, addr, Inv, GetData, NotFound};
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
	fn write_message(self, payload: Payload) -> WriteMessage<Connection<A>> {
		let message = Message::new(self.magic, payload);
		write_message(self, message)
	}

	pub fn ping(self) -> WriteMessage<Connection<A>> {
		let payload = Payload::Ping(Ping {
			nonce: 0,
		});
		self.write_message(payload)
	}

	pub fn pong(self, nonce: u64) -> WriteMessage<Connection<A>> {
		let payload = Payload::Pong(Pong {
			nonce: nonce,
		});
		self.write_message(payload)
	}

	pub fn getaddr(self) -> WriteMessage<Connection<A>> {
		let payload = Payload::GetAddr;
		self.write_message(payload)
	}

	pub fn addr(self, addresses: Vec<addr::AddressEntry>) -> WriteMessage<Connection<A>> {
		let payload = if self.handshake_result.negotiated_version < 31402 {
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

	pub fn inv(self, inventory: Vec<InventoryVector>) -> WriteMessage<Connection<A>> {
		let payload = Payload::Inv(Inv {
			inventory: inventory,
		});
		self.write_message(payload)
	}

	pub fn getdata(self, inventory: Vec<InventoryVector>) -> WriteMessage<Connection<A>> {
		let payload = Payload::GetData(GetData {
			inventory: inventory,
		});
		self.write_message(payload)
	}

	pub fn notfound(self, inventory: Vec<InventoryVector>) -> WriteMessage<Connection<A>> {
		let payload = Payload::NotFound(NotFound {
			inventory: inventory,
		});
		self.write_message(payload)
	}
}
