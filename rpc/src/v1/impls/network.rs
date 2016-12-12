use v1::traits::Network as NetworkRpc;
use v1::types::AddNodeOperation;
use jsonrpc_core::Error;
use v1::helpers::errors;
use std::net::SocketAddr;
use p2p;

pub trait NetworkApi : Send + Sync + 'static {
	fn add_node(&self, socket_addr: SocketAddr) -> Result<(), p2p::NodeTableError>;
	fn remove_node(&self, socket_addr: SocketAddr) -> Result<(), p2p::NodeTableError>;
	fn connect(&self, socket_addr: SocketAddr);
}

impl<T> NetworkRpc for NetworkClient<T> where T: NetworkApi {
	fn add_node(&self, node: String, operation: AddNodeOperation) -> Result<(), Error> {
		let addr = try!(node.parse().map_err(
			|_| errors::invalid_params("node", "Invalid socket address format, should be ip:port (127.0.0.1:8008)")));
		match operation {
			AddNodeOperation::Add => {
				self.api.add_node(addr).map_err(|_| errors::node_already_added())
			},
			AddNodeOperation::Remove => {
				self.api.remove_node(addr).map_err(|_| errors::node_not_added())
			},
			AddNodeOperation::OneTry => {
				self.api.connect(addr);
				Ok(())
			}
		}
	}
}

pub struct NetworkClient<T: NetworkApi> {
	api: T,
}

impl<T> NetworkClient<T> where T: NetworkApi {
	pub fn new(api: T) -> Self {
		NetworkClient {
			api: api,
		}
	}
}


