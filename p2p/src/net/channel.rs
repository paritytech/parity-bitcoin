use std::io;
use futures::Poll;
use futures::stream::Stream;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{MessageResult, PayloadType};
use message::common::Command;
use net::Connection;
use io::{read_message_stream, ReadMessageStream, SharedTcpStream, WriteMessage};

pub struct Channel {
	connection: Connection,
	message_stream: Mutex<ReadMessageStream<SharedTcpStream>>,
}

impl Channel {
	pub fn new(connection: Connection) -> Self {
		let stream = read_message_stream(connection.stream.clone(), connection.magic);
		Channel {
			connection: connection,
			message_stream: Mutex::new(stream),
		}
	}

	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: PayloadType {
		self.connection.write_message(payload)
	}

	pub fn poll_message(&self) -> Poll<Option<(MessageResult<(Command, Bytes)>)>, io::Error> {
		self.message_stream.lock().poll()
	}

	pub fn version(&self) -> u32 {
		self.connection.version
	}
}
