use std::net::SocketAddr;
use message::types;
use network::Magic;

pub type PeerId = usize;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Direction {
	Inbound,
	Outbound,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PeerInfo {
	pub id: PeerId,
	pub address: SocketAddr,
	pub user_agent: String,
	pub direction: Direction,
	pub version: u32,
	pub version_message: types::Version,
	pub magic: Magic,
}

