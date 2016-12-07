use jsonrpc_core::Error;

use v1::helpers::auto_args::Wrap;
use v1::types::RawTransaction;

build_rpc_trait! {
	/// Eth rpc interface.
	pub trait Raw {
		/// Returns protocol version encoded as a string (quotes are necessary).
		#[rpc(name = "sendrawtransaction")]
		fn send_raw_transaction(&self, RawTransaction) -> Result<String, Error>;
	}
}
