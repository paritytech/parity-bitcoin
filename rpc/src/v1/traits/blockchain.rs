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
		fn difficulty(&self) -> Result<f64, Error>;
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
/*
use db::IndexedBlock;
use primitives::hash::H256;
use primitives::uint::U256;

/// Verbose block information
#[derive(Debug)]
pub struct VerboseBlock {
	/// Block
	pub block: IndexedBlock,
	/// Number of confirmations. -1 if block is on the side chain
	pub confirmations: i64,
	/// Block size
	pub size: u32,
	/// Block size, excluding witness data
	pub stripped_size: u32,
	/// Block weight
	pub weight: u32,
	/// Block height. We only provide this for main chain blocks
	pub height: Option<u32>,
	/// Median block time in seconds since epoch (Jan 1 1970 GMT)
	/// We only provide this when there are > 2 parent blocks
	pub median_time: Option<u32>,
	/// Block difficulty
	pub difficulty: f64,
	/// Expected number of hashes required to produce the chain up to this block
	pub chain_work: Option<U256>,
	/// Hash of previous block
	pub previous_block_hash: Option<H256>,
	/// Hash of next block
	pub next_block_hash: Option<H256>,
}

*/