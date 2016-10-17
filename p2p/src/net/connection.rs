use std::net;
use message::Magic;
use io::SharedTcpStream;

pub struct Connection {
	pub stream: SharedTcpStream,
	pub version: u32,
	pub magic: Magic,
	pub address: net::SocketAddr,
}
