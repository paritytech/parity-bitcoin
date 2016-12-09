use jsonrpc_core::Error;

use v1::helpers::auto_args::Wrap;
use v1::types::{BlockTemplate, BlockTemplateRequest};

build_rpc_trait! {
	/// Partiy-bitcoin miner data interface.
	pub trait Miner {
		/// Get block template for mining.
		#[rpc(name = "getblocktemplate")]
		fn get_block_template(&self, BlockTemplateRequest) -> Result<BlockTemplate, Error>;
	}
}
