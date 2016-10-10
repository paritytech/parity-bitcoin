use std::net;
use message::{Message, PayloadType, MessageResult};
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
	pub fn write_message<T>(&self, payload: &T) -> MessageResult<WriteMessage<T, &TcpStream>> where T: PayloadType {
		let message = try!(Message::new(self.magic, self.version, payload));
		Ok(write_message(&self.stream, message))
	}
}
