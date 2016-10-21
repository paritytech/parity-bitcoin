//! Bitcoin chain verifier

use std::sync::Arc;

use db;
use chain;
use super::{Verify, VerificationResult, Chain};

struct ChainVerifier {
	store: Arc<db::Store>,
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {
		Ok(Chain::Main)
	}
}

#[cfg(test)]
mod tests {

	use primitives::hash::H256;
	use chain;
	use std::collections::HashMap;
	use db::{self, BlockRef, Store};
	use serialization;
	use serialization::bytes::Bytes;

	struct TestStorage {
		blocks: HashMap<H256, chain::Block>,
		heights: HashMap<usize, H256>,
	}

	impl TestStorage {
		fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
			match block_ref {
				BlockRef::Number(n) => self.block_hash(n),
				BlockRef::Hash(h) => Some(h),
			}
		}
	}

	impl Store for TestStorage {
		fn block_hash(&self, number: u64) -> Option<H256> {
			self.heights.get(&(number as usize)).cloned()
		}

		fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.map(|ref block| serialization::serialize(block.header()))
		}

		fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.map(|ref block| block.transactions().iter().map(|tx| tx.hash()).collect())
				.unwrap_or(Vec::new())
		}

		fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
			self.transaction(hash).map(|tx| serialization::serialize(&tx))
		}

		fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
			self.blocks.iter().flat_map(|(_, b)| b.transactions())
				.find(|ref tx| tx.hash() == *hash)
				.cloned()
		}

		fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
			self.blocks.iter().flat_map(|(_, b)| b.transactions()).cloned().collect()
		}

		fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.cloned()
		}

		fn insert_block(&self, block: &chain::Block) -> Result<(), db::Error> {
			Ok(())
		}
	}


}
