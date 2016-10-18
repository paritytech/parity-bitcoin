use std::io;
use futures::Poll;
use futures::stream::Stream;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{MessageResult, Payload, Command, Magic, Message};
use net::Connection;
use io::{read_message_stream, ReadMessageStream, SharedTcpStream, WriteMessage, write_message};
use {PeerId, PeerInfo};

pub struct Channel {
	version: u32,
	magic: Magic,
	peer_info: PeerInfo,
	write_stream: SharedTcpStream,
	read_stream: Mutex<ReadMessageStream<SharedTcpStream>>,
}

impl Channel {
	pub fn new(connection: Connection, peer_id: PeerId) -> Self {
		let stream = read_message_stream(connection.stream.clone(), connection.magic);
		Channel {
			version: connection.version,
			magic: connection.magic,
			peer_info: PeerInfo {
				address: connection.address,
				id: peer_id,
			},
			write_stream: connection.stream,
			read_stream: Mutex::new(stream),
		}
	}

	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: Payload {
		// TODO: some tracing here
		let message = Message::new(self.magic, self.version, payload).expect("failed to create outgoing message");
		write_message(self.write_stream.clone(), message)
	}

	pub fn poll_message(&self) -> Poll<Option<(MessageResult<(Command, Bytes)>)>, io::Error> {
		self.read_stream.lock().poll()
	}

	pub fn shutdown(&self) {
		self.write_stream.shutdown();
	}

	pub fn version(&self) -> u32 {
		self.version
	}

	pub fn peer_info(&self) -> PeerInfo {
		self.peer_info
	}
}
