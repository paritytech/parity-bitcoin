use std::net;
use message::{Message, PayloadType};
use message::common::Magic;
use tokio_core::net::TcpStream;
use io::{write_message, WriteMessage};

pub struct Connection {
	pub stream: TcpStream,
	pub version: u32,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

impl Connection {
	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, &TcpStream> where T: PayloadType {
		let message = match Message::new(self.magic, self.version, payload) {
			Ok(message) => message,
			Err(_err) => {
				// trace here! outgoing messages should always be written properly
				panic!();
			}
		};
		write_message(&self.stream, message)
	}
}
