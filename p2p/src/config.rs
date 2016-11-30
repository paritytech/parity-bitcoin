use std::{net, path};
use net::Config as NetConfig;
use util::InternetProtocol;

#[derive(Debug, Clone)]
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
	pub peers: Vec<net::SocketAddr>,
	/// Connect to these nodes to retrieve peer addresses, and disconnect.
	pub seeds: Vec<String>,
	/// p2p/nodes.csv file path
	pub node_table_path: path::PathBuf,
	/// Internet protocol.
	pub internet_protocol: InternetProtocol,
}
