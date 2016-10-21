use tokio_core::io::{write_all, WriteAll};
use bytes::Bytes;
use message::{Payload, Magic, Message, to_raw_message, Command};
use net::Connection;
use session::Session;
use io::{SharedTcpStream, WriteMessage, write_message, read_any_message, ReadAnyMessage};
use {PeerId, PeerInfo};

pub struct Channel {
	version: u32,
	magic: Magic,
	peer_info: PeerInfo,
	session: Session,
	stream: SharedTcpStream,
}

impl Channel {
	pub fn new(connection: Connection, peer_id: PeerId, session: Session) -> Self {
		Channel {
			version: connection.version,
			magic: connection.magic,
			peer_info: PeerInfo {
				address: connection.address,
				id: peer_id,
			},
			session: session,
			stream: connection.stream,
		}
	}

	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: Payload {
		// TODO: some tracing here
		let message = Message::new(self.magic, self.version, payload).expect("failed to create outgoing message");
		write_message(self.stream.clone(), message)
	}

	pub fn write_raw_message(&self, command: Command, payload: &Bytes) -> WriteAll<SharedTcpStream, Bytes> {
		let message = to_raw_message(self.magic, command, payload);
		write_all(self.stream.clone(), message)
	}

	pub fn read_message(&self) -> ReadAnyMessage<SharedTcpStream> {
		read_any_message(self.stream.clone(), self.magic)
	}

	pub fn shutdown(&self) {
		self.stream.shutdown();
	}

	pub fn version(&self) -> u32 {
		self.version
	}

	pub fn peer_info(&self) -> PeerInfo {
		self.peer_info
	}

	pub fn session(&self) -> &Session {
		&self.session
	}
}
