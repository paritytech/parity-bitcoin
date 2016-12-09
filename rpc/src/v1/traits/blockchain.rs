use jsonrpc_core::Error;

use v1::helpers::auto_args::Wrap;
use v1::types::H256;
use v1::types::GetBlockResponse;
use v1::types::GetTransactionResponse;
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
		fn difficulty(&self) -> Result<u32, Error>;
		/// Get information on given block.
		#[rpc(name = "getblock")]
		fn block(&self, H256, Option<bool>) -> Result<GetBlockResponse, Error>;
		/// Get information on given transaction.
		#[rpc(name = "gettransaction")]
		fn transaction(&self, H256, Option<bool>) -> Result<GetTransactionResponse, Error>;
		/// Get details about an unspent transaction output.
		#[rpc(name = "gettxout")]
		fn transaction_out(&self, H256, u32, Option<bool>) -> Result<GetTxOutResponse, Error>;
		/// Get statistics about the unspent transaction output set.
		#[rpc(name = "gettxoutsetinfo")]
		fn transaction_out_set_info(&self) -> Result<GetTxOutSetInfoResponse, Error>;
	}
}
