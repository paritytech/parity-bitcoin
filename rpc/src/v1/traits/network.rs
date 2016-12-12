use jsonrpc_core::Error;
use v1::types::AddNodeOperation;

build_rpc_trait! {
	/// Parity-bitcoin network interface
	pub trait Network {
		/// Add/remove/connecto to the node
		#[rpc(name = "addnode")]
		fn add_node(&self, String, AddNodeOperation) -> Result<(), Error>;
	}
}
