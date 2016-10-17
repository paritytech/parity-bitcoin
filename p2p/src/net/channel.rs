use std::{io, net};
use futures::Poll;
use futures::stream::Stream;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{MessageResult, Payload, Command, Magic, Message};
use net::Connection;
use io::{read_message_stream, ReadMessageStream, SharedTcpStream, WriteMessage, write_message};

pub struct Channel {
	write_stream: SharedTcpStream,
	version: u32,
	magic: Magic,
	address: net::SocketAddr,
	read_stream: Mutex<ReadMessageStream<SharedTcpStream>>,
}

impl Channel {
	pub fn new(connection: Connection) -> Self {
		let stream = read_message_stream(connection.stream.clone(), connection.magic);
		Channel {
			write_stream: connection.stream,
			version: connection.version,
			magic: connection.magic,
			address: connection.address,
			read_stream: Mutex::new(stream),
		}
	}

	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: Payload {
		let message = Message::new(self.magic, self.version, payload).expect("failed to create outgoing message");
		write_message(self.write_stream.clone(), message)
	}

	pub fn poll_message(&self) -> Poll<Option<(MessageResult<(Command, Bytes)>)>, io::Error> {
		self.read_stream.lock().poll()
	}

	pub fn version(&self) -> u32 {
		self.version
	}
}
