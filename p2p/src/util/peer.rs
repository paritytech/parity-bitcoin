use std::net::SocketAddr;
use message::Magic;

pub type PeerId = usize;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Direction {
	Inbound,
	Outbound,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PeerInfo {
	pub id: PeerId,
	pub address: SocketAddr,
	pub direction: Direction,
	pub version: u32,
	pub magic: Magic,
}

