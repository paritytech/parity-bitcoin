use std::net;
use network::Magic;
use message::common::Services;
use message::types;
use io::SharedTcpStream;

pub struct Connection {
	pub stream: SharedTcpStream,
	pub version: u32,
	pub version_message: types::Version,
	pub magic: Magic,
	pub services: Services,
	pub address: net::SocketAddr,
}
