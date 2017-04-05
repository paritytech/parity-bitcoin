use tokio_io::io::{write_all, WriteAll};
use session::Session;
use io::{SharedTcpStream, read_any_message, ReadAnyMessage};
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

	pub fn write_message<T>(&self, message: T) -> WriteAll<SharedTcpStream, T> where T: AsRef<[u8]> {
		write_all(self.stream.clone(), message)
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
