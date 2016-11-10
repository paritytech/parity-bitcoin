use message::{Payload, Message};
use session::Session;
use io::{SharedTcpStream, WriteMessage, write_message, read_any_message, ReadAnyMessage};
use util::PeerInfo;

pub struct Channel {
	stream: SharedTcpStream,
	peer_info: PeerInfo,
	session: Session,
}

impl Channel {
	pub fn new(stream: SharedTcpStream, peer_info: PeerInfo, session: Session) -> Self {
		Channel {
			stream: stream,
			peer_info: peer_info,
			session: session,
		}
	}

	pub fn write_message<T>(&self, payload: &T) -> WriteMessage<T, SharedTcpStream> where T: Payload {
		// TODO: some tracing here
		let message = Message::new(self.peer_info.magic, self.peer_info.version, payload).expect("failed to create outgoing message");
		write_message(self.stream.clone(), message)
	}

	pub fn read_message(&self) -> ReadAnyMessage<SharedTcpStream> {
		read_any_message(self.stream.clone(), self.peer_info.magic)
	}

	pub fn shutdown(&self) {
		self.stream.shutdown();
	}

	pub fn version(&self) -> u32 {
		self.peer_info.version
	}

	pub fn peer_info(&self) -> PeerInfo {
		self.peer_info.clone()
	}

	pub fn session(&self) -> &Session {
		&self.session
	}
}
