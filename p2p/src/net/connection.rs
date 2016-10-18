use std::net;
use message::Magic;
use message::common::Services;
use io::SharedTcpStream;

pub struct Connection {
	pub stream: SharedTcpStream,
	pub version: u32,
	pub magic: Magic,
	pub services: Services,
	pub address: net::SocketAddr,
}
