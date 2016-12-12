use jsonrpc_macros::Trailing;
use jsonrpc_core::Error;

use v1::types::H256;
use v1::types::GetBlockResponse;
use v1::types::GetTxOutResponse;
use v1::types::GetTxOutSetInfoResponse;


build_rpc_trait! {
	/// Parity-bitcoin blockchain data interface.
	pub trait BlockChain {
		/// Get hash of best block.
		#[rpc(name = "getbestblockhash")]
		fn best_block_hash(&self) -> Result<H256, Error>;
		/// Get hash of block at given height.
		#[rpc(name = "getblockhash")]
		fn block_hash(&self, u32) -> Result<H256, Error>;
		/// Get proof-of-work difficulty as a multiple of the minimum difficulty
		#[rpc(name = "getdifficulty")]
		fn difficulty(&self) -> Result<f64, Error>;
		/// Get information on given block.
		#[rpc(name = "getblock")]
		fn block(&self, H256, Trailing<bool>) -> Result<GetBlockResponse, Error>;
		/// Get details about an unspent transaction output.
		#[rpc(name = "gettxout")]
		fn transaction_out(&self, H256, u32, Trailing<bool>) -> Result<GetTxOutResponse, Error>;
		/// Get statistics about the unspent transaction output set.
		#[rpc(name = "gettxoutsetinfo")]
		fn transaction_out_set_info(&self) -> Result<GetTxOutSetInfoResponse, Error>;
	}
}
