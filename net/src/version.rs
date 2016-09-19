use {ServiceFlags, NetAddress};

pub enum Version {
	Simple(Simple),
	V106(V106),
	V70001(V70001),
}

pub struct Simple {
	pub version: u32,
	pub services: ServiceFlags,
	pub timestamp: i64,
	pub receiver: NetAddress,
}

pub struct V106 {
	pub version: u32,
	pub services: ServiceFlags,
	pub timestamp: i64,
	pub receiver: NetAddress,
	pub from: NetAddress,
	pub nonce: u64,
	// TODO: read str
}

pub struct V70001 {
	pub version: u32,
	pub services: ServiceFlags,
	pub timestamp: i64,
	pub receiver: NetAddress,
	pub from: NetAddress,
	pub nonce: u64,
	// TODO: read str
}
