use std::net;
use message::{Message, Payload};
use message::common::Magic;
use tokio_core::net::TcpStream;
use io::{write_message, WriteMessage, read_message_stream, ReadMessageStream};

pub struct Connection {
	pub stream: TcpStream,
	pub version: u32,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

impl Connection {
	pub fn incoming(&self) -> ReadMessageStream<&TcpStream> {
		read_message_stream(&self.stream, self.magic, self.version)
	}
}

impl Connection {
	pub fn write_message(&self, payload: Payload) -> WriteMessage<&TcpStream> {
		let message = Message::new(self.magic, payload);
		write_message(&self.stream, message)
	}
}
