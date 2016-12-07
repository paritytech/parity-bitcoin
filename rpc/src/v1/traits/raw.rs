use jsonrpc_core::Error;

use v1::helpers::auto_args::Wrap;
use v1::types::RawTransaction;

build_rpc_trait! {
	/// Partiy-bitcoin raw data interface.
	pub trait Raw {
		/// Adds transaction to the memory pool && relays it to the peers.
		#[rpc(name = "sendrawtransaction")]
		fn send_raw_transaction(&self, RawTransaction) -> Result<(), Error>;
	}
}
