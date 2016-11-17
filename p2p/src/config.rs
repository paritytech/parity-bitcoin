use std::net::SocketAddr;
use std::path::PathBuf;
use net::Config as NetConfig;

#[derive(Debug)]
pub struct Config {
	/// Number of threads used by p2p thread pool.
	pub threads: usize,
	/// Number of inbound connections.
	pub inbound_connections: u32,
	/// Number of outbound connections.
	pub outbound_connections: u32,
	/// Configuration for every connection.
	pub connection: NetConfig,
	/// Connect only ot these nodes.
	pub peers: Vec<SocketAddr>,
	/// Connect to these nodes to retrieve peer addresses, and disconnect.
	pub seeds: Vec<String>,
	/// p2p module cache directory.
	pub node_table_path: PathBuf,
}
