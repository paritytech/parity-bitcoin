use std::{net, io};
use message::common::Magic;
use io::HandshakeResult;

pub struct Connection<A> where A: io::Read + io::Write {
	pub stream: A,
	pub handshake_result: HandshakeResult,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

