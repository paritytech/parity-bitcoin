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
			self.heights.get(&(number as usize)).map(|h| h.clone())
		}

		fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
			None
		}

		fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
			Vec::new()
		}

		fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
			None
		}

		fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
			None
		}

		fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
			Vec::new()
		}

		fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
			None
		}

		fn insert_block(&self, block: &chain::Block) -> Result<(), db::Error> {
			Ok(())
		}
	}


}
