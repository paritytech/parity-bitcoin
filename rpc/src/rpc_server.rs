// TODO: panic handler
use std::io;
use std::net::SocketAddr;
use jsonrpc_core;
use jsonrpc_http_server::{self, ServerBuilder, Server, Host};

/// Start http server asynchronously and returns result with `Server` handle on success or an error.
pub fn start_http<M: jsonrpc_core::Metadata>(
	addr: &SocketAddr,
	cors_domains: Option<Vec<String>>,
	allowed_hosts: Option<Vec<String>>,
	handler: jsonrpc_core::MetaIoHandler<M>,
	) -> Result<Server, io::Error> {

	let cors_domains = cors_domains.map(|domains| {
		domains.into_iter()
			.map(|v| match v.as_str() {
				"*" => jsonrpc_http_server::AccessControlAllowOrigin::Any,
				"null" => jsonrpc_http_server::AccessControlAllowOrigin::Null,
				v => jsonrpc_http_server::AccessControlAllowOrigin::Value(v.into()),
			})
			.collect()
	});

	ServerBuilder::new(handler)
		.cors(cors_domains.into())
		.allowed_hosts(allowed_hosts.map(|hosts| hosts.into_iter().map(Host::from).collect()).into())
		.start_http(addr)
}
