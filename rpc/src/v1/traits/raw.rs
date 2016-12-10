use jsonrpc_core::Error;

use v1::types::RawTransaction;
use v1::types::H256;

build_rpc_trait! {
	/// Partiy-bitcoin raw data interface.
	pub trait Raw {
		/// Adds transaction to the memory pool && relays it to the peers.
		#[rpc(name = "sendrawtransaction")]
		fn send_raw_transaction(&self, RawTransaction) -> Result<H256, Error>;
	}
}
