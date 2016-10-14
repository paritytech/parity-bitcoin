use std::net;
use message::{Message, Payload, Magic};
use io::{write_message, WriteMessage, SharedTcpStream};

pub struct Connection {
	pub stream: SharedTcpStream,
	pub version: u32,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

impl Connection {
	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: Payload {
		let message = match Message::new(self.magic, self.version, payload) {
			Ok(message) => message,
			Err(_err) => {
				// trace here! outgoing messages should always be written properly
				panic!();
			}
		};
		write_message(self.stream.clone(), message)
	}
}
