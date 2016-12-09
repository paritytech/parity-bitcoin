// TODO: panic handler
use std::sync::Arc;
use std::net::SocketAddr;
use jsonrpc_core::{IoHandler, IoDelegate};
use jsonrpc_http_server::{self, ServerBuilder, Server, RpcServerError};

/// An object that can be extended with `IoDelegates`
pub trait Extendable {
	/// Add `Delegate` to this object.
	fn add_delegate<D: Send + Sync + 'static>(&self, delegate: IoDelegate<D>);
}

/// Http server.
pub struct RpcServer {
	handler: Arc<IoHandler>,
}

impl Extendable for RpcServer {
	/// Add io delegate.
	fn add_delegate<D: Send + Sync + 'static>(&self, delegate: IoDelegate<D>) {
		self.handler.add_delegate(delegate);
	}
}

impl RpcServer {
	/// Construct new http server object.
	pub fn new() -> RpcServer {
		RpcServer {
			handler: Arc::new(IoHandler::new()),
		}
	}

	/// Start http server asynchronously and returns result with `Server` handle on success or an error.
	pub fn start_http(
		&self,
		addr: &SocketAddr,
		cors_domains: Option<Vec<String>>,
		allowed_hosts: Option<Vec<String>>,
		) -> Result<Server, RpcServerError> {

		let cors_domains = cors_domains.map(|domains| {
			domains.into_iter()
				.map(|v| match v.as_str() {
					"*" => jsonrpc_http_server::AccessControlAllowOrigin::Any,
					"null" => jsonrpc_http_server::AccessControlAllowOrigin::Null,
					v => jsonrpc_http_server::AccessControlAllowOrigin::Value(v.into()),
				})
				.collect()
		});

		ServerBuilder::new(self.handler.clone())
			.cors(cors_domains.into())
			.allowed_hosts(allowed_hosts.into())
			.start_http(addr)
	}
}
