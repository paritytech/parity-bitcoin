use std::net::SocketAddr;
use network::Magic;
use message::common::{Services, NetAddress};
use message::types::version::{Version, V0, V106, V70001};
use util::time::{Time, RealTime};
use util::nonce::{NonceGenerator, RandomNonce};

#[derive(Debug, Clone)]
pub struct Config {
	pub protocol_version: u32,
	pub protocol_minimum: u32,
	pub magic: Magic,
	pub local_address: SocketAddr,
	pub services: Services,
	pub user_agent: String,
	pub start_height: i32,
	pub relay: bool,
}

impl Config {
	pub fn version(&self, to: &SocketAddr) -> Version {
		Version::V70001(V0 {
			version: self.protocol_version,
			services: self.services,
			timestamp: RealTime.get().sec,
			receiver: NetAddress {
				services: self.services,
				address: to.ip().into(),
				port: to.port().into(),
			},
		}, V106 {
			from: NetAddress {
				services: self.services,
				address: self.local_address.ip().into(),
				port: self.local_address.port().into(),
			},
			nonce: RandomNonce.get(),
			user_agent: self.user_agent.clone(),
			start_height: self.start_height,
		}, V70001 {
			relay: self.relay,
		})
	}
}
