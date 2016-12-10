use v1::traits::BlockChain;
use v1::types::{GetBlockResponse, VerboseBlock};
use v1::types::GetTransactionResponse;
use v1::types::GetTxOutResponse;
use v1::types::GetTxOutSetInfoResponse;
use v1::types::RawBlock;
use v1::types::H256;
use v1::types::U256;
use jsonrpc_core::Error;
use db;
use verification;
use ser::serialize;
use primitives::hash::H256 as GlobalH256;

pub struct BlockChainClient<T: BlockChainClientCoreApi> {
	_core: T,
}

pub trait BlockChainClientCoreApi: Send + Sync + 'static {
	fn get_raw_block(&self, hash: GlobalH256) -> Option<RawBlock>;
	fn get_verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock>;
}

pub struct BlockChainClientCore {
	storage: db::SharedStore,
}

impl BlockChainClientCore {
	pub fn new(storage: db::SharedStore) -> Self {
		BlockChainClientCore {
			storage: storage,
		}
	}
}

impl BlockChainClientCoreApi for BlockChainClientCore {
	fn get_raw_block(&self, hash: GlobalH256) -> Option<RawBlock> {
		self.storage.block(hash.into())
			.map(|block| {
				serialize(&block).into()
			})
	}

	fn get_verbose_block(&self, hash: GlobalH256) -> Option<VerboseBlock> {
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
			_core: core,
		}
	}
}

impl<T> BlockChain for BlockChainClient<T> where T: BlockChainClientCoreApi {
	fn best_block_hash(&self) -> Result<H256, Error> {
		rpc_unimplemented!()
	}

	fn block_hash(&self, _height: u32) -> Result<H256, Error> {
		rpc_unimplemented!()
	}

	fn difficulty(&self) -> Result<f64, Error> {
		rpc_unimplemented!()
	}

	fn block(&self, _hash: H256, _verbose: Option<bool>) -> Result<GetBlockResponse, Error> {
		rpc_unimplemented!()
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
}
