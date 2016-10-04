use std::net::IpAddr;
use net::Config as NetConfig;

#[derive(Debug)]
pub struct Config {
	/// Configuration for every connection.
	pub connection: NetConfig,
	/// Connect to these nodes to retrieve peer addresses, and disconnect.
	pub seednodes: Vec<IpAddr>,
	/// Connect only ot these nodes.
	pub limited_connect: Option<Vec<IpAddr>>,
}
