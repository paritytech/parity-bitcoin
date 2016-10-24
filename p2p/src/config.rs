use std::net::IpAddr;
use net::Config as NetConfig;

#[derive(Debug)]
pub struct Config {
	/// Number of threads used by p2p thread pool.
	pub threads: usize,
	/// Lowest supported protocol version.
	pub protocol_minimum: u32,
	/// Highest supported protocol version.
	pub protocol_maximum: u32,
	/// Number of inbound connections.
	pub inbound_connections: usize,
	/// Number of outbound connections.
	pub outbound_connections: usize,
	/// Configuration for every connection.
	pub connection: NetConfig,
	/// Connect only ot these nodes.
	pub peers: Vec<IpAddr>,
	/// Connect to these nodes to retrieve peer addresses, and disconnect.
	pub seeds: Vec<String>,
}
