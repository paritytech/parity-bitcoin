use v1::traits::BlockChain;
use v1::types::{GetBlockResponse, VerboseBlock, RawBlock};
use v1::types::GetTransactionResponse;
use v1::types::GetTxOutResponse;
use v1::types::GetTxOutSetInfoResponse;
use v1::types::H256;
use v1::types::U256;
use v1::helpers::errors::{block_not_found, block_at_height_not_found};
use jsonrpc_core::Error;
use db;
use verification;
use ser::serialize;
use primitives::hash::H256 as GlobalH256;

pub struct BlockChainClient<T: BlockChainClientCoreApi> {
	core: T,
}

pub trait BlockChainClientCoreApi: Send + Sync + 'static {
	fn best_block_hash(&self) -> GlobalH256;
	fn block_hash(&self, height: u32) -> Option<GlobalH256>;
	fn difficulty(&self) -> f64;
	fn raw_block(&self, hash: GlobalH256) -> Option<RawBlock>;
	fn verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock>;
}

pub struct BlockChainClientCore {
	storage: db::SharedStore,
}

impl BlockChainClientCore {
	pub fn new(storage: db::SharedStore) -> Self {
		assert!(storage.best_block().is_some());
		
		BlockChainClientCore {
			storage: storage,
		}
	}
}

impl BlockChainClientCoreApi for BlockChainClientCore {
	fn best_block_hash(&self) -> GlobalH256 {
		self.storage.best_block().expect("storage with genesis block required").hash
	}

	fn block_hash(&self, height: u32) -> Option<GlobalH256> {
		self.storage.block_hash(height)
	}

	fn difficulty(&self) -> f64 {
		unimplemented!()
	}

	fn raw_block(&self, hash: GlobalH256) -> Option<RawBlock> {
		self.storage.block(hash.into())
			.map(|block| {
				serialize(&block).into()
			})
	}

	fn verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock> {
		self.storage.block(hash.into())
			.map(|block| {
				let block: db::IndexedBlock = block.into();
				let height = self.storage.block_number(block.hash());
				let confirmations = match height {
					Some(block_number) => (self.storage.best_block().expect("genesis block is required").number - block_number + 1) as i64,
					None => -1,
				};
				let block_size = block.size();
				let median_time = verification::ChainVerifier::median_timestamp(self.storage.as_block_header_provider(), block.header());
				VerboseBlock {
					confirmations: confirmations,
					size: block_size as u32,
					strippedsize: block_size as u32, // TODO: segwit
					weight: block_size as u32, // TODO: segwit
					height: height,
					mediantime: median_time,
					difficulty: 0f64, // TODO: https://en.bitcoin.it/wiki/Difficulty + https://www.bitcoinmining.com/what-is-bitcoin-mining-difficulty/
					chainwork: U256::default(), // TODO: read from storage
					previousblockhash: Some(block.header().previous_header_hash.clone().into()),
					nextblockhash: height.and_then(|h| self.storage.block_hash(h + 1).map(|h| h.into())),
					bits: block.header().bits.into(),
					hash: block.hash().clone().reversed().into(),
					merkleroot: block.header().merkle_root_hash.clone().into(),
					nonce: block.header().nonce,
					time: block.header().time,
					tx: vec![], // TODO
					version: block.header().version,
					version_hex: format!("{:x}", block.header().version),
				}
			})
	}
}

impl<T> BlockChainClient<T> where T: BlockChainClientCoreApi {
	pub fn new(core: T) -> Self {
		BlockChainClient {
			core: core,
		}
	}
}

impl<T> BlockChain for BlockChainClient<T> where T: BlockChainClientCoreApi {
	fn best_block_hash(&self) -> Result<H256, Error> {
		Ok(self.core.best_block_hash().into())
	}

	fn block_hash(&self, height: u32) -> Result<H256, Error> {
		self.core.block_hash(height)
			.map(H256::from)
			.ok_or(block_at_height_not_found(height))
	}

	fn difficulty(&self) -> Result<f64, Error> {
		Ok(self.core.difficulty())
	}

	fn block(&self, hash: H256, verbose: Option<bool>) -> Result<GetBlockResponse, Error> {
		let global_hash: GlobalH256 = hash.clone().into();
		if verbose.unwrap_or_default() {
			self.core.verbose_block(global_hash.reversed())
				.map(|block| GetBlockResponse::Verbose(block))
		} else {
			self.core.raw_block(global_hash.reversed())
				.map(|block| GetBlockResponse::Raw(block))
		}
		.ok_or(block_not_found(hash))
	}

	fn transaction(&self, _hash: H256, _watch_only: Option<bool>) -> Result<GetTransactionResponse, Error> {
		rpc_unimplemented!()
	}

	fn transaction_out(&self, _transaction_hash: H256, _out_index: u32, _include_mempool: Option<bool>) -> Result<GetTxOutResponse, Error> {
		rpc_unimplemented!()
	}

	fn transaction_out_set_info(&self) -> Result<GetTxOutSetInfoResponse, Error> {
		rpc_unimplemented!()
	}
}

#[cfg(test)]
pub mod tests {
	use primitives::hash::H256 as GlobalH256;
	use v1::types::{VerboseBlock, RawBlock};
	use test_data;
	use super::*;

	#[derive(Default)]
	struct SuccessBlockChainClientCore;
	#[derive(Default)]
	struct ErrorBlockChainClientCore;

	impl BlockChainClientCoreApi for SuccessBlockChainClientCore {
		fn best_block_hash(&self) -> GlobalH256 {
			test_data::genesis().hash()
		}

		fn block_hash(&self, _height: u32) -> Option<GlobalH256> {
			Some(test_data::genesis().hash())
		}

		fn difficulty(&self) -> f64 {
			1f64
		}

		fn raw_block(&self, _hash: GlobalH256) -> Option<RawBlock> {
			Some(RawBlock::from(vec![0]))
		}

		fn verbose_block(&self, _hash: GlobalH256) -> Option<VerboseBlock> {
			Some(VerboseBlock::default())
		}
	}

	impl BlockChainClientCoreApi for ErrorBlockChainClientCore {
		fn best_block_hash(&self) -> GlobalH256 {
			test_data::genesis().hash()
		}

		fn block_hash(&self, _height: u32) -> Option<GlobalH256> {
			None
		}

		fn difficulty(&self) -> f64 {
			1f64
		}

		fn raw_block(&self, _hash: GlobalH256) -> Option<RawBlock> {
			None
		}

		fn verbose_block(&self, _hash: GlobalH256) -> Option<VerboseBlock> {
			None
		}
	}

	#[test]
	fn best_block_hash_success() {
		// TODO
	}

	#[test]
	fn block_hash_success() {
		// TODO
	}

	#[test]
	fn block_hash_error() {
		// TODO
	}

	#[test]
	fn difficulty_success() {
		// TODO
	}

	#[test]
	fn verbose_block_contents() {
		// TODO
	}

	#[test]
	fn raw_block_success() {
		// TODO
	}

	#[test]
	fn raw_block_error() {
		// TODO
	}

	#[test]
	fn verbose_block_success() {
		// TODO
	}

	#[test]
	fn verbose_block_error() {
		// TODO
	}
}
