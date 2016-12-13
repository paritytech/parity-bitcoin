use jsonrpc_core::Error;
use jsonrpc_macros::Trailing;
use v1::types::{AddNodeOperation, NodeInfo};

build_rpc_trait! {
	/// Parity-bitcoin network interface
	pub trait Network {
		/// Add/remove/connecto to the node
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "add"], "id":1 }' -H 'content-type: application/json;' http://127.0.0.1:8332/
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "remove"], "id":1 }' -H 'content-type: application/json;' http://127.0.0.1:8332/
		/// @curl-example: curl --data-binary '{"jsonrpc": "2.0", "method": "addnode", "params": ["127.0.0.1:8888", "onetry"], "id":1 }' -H 'content-type: application/json;' http://127.0.0.1:8332/
		#[rpc(name = "addnode")]
		fn add_node(&self, String, AddNodeOperation) -> Result<(), Error>;
		/// Query node(s) info
		/// @curl-example: curl --data-binary '{"jsonrpc": "1.0", "id":"curltest", "method": "getaddednodeinfo", "params": [true] }' -H 'content-type: application/json;' http://127.0.0.1:8332/
		/// @curl-example: curl --data-binary '{"jsonrpc": "1.0", "id":"curltest", "method": "getaddednodeinfo", "params": [true, "192.168.0.201"] }' -H 'content-type: application/json;' http://127.0.0.1:8332/
		#[rpc(name = "getaddednodeinfo")]
		fn node_info(&self, bool, Trailing<String>) -> Result<Vec<NodeInfo>, Error>;
	}
}
