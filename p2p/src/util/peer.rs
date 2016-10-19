use std::net::SocketAddr;

pub type PeerId = usize;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct PeerInfo {
	pub id: PeerId,
	pub address: SocketAddr,
}

