//! Bitcoin chain verifier

use std::sync::Arc;

use db::{self, BlockRef};
use chain;
use super::{Verify, VerificationResult, Chain, Error};

pub struct ChainVerifier {
	store: Arc<db::Store>,
}

impl ChainVerifier {
	pub fn new(store: Arc<db::Store>) -> Self {
		ChainVerifier { store: store }
	}
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {

		// There should be at least 1 transaction
		if block.transactions().is_empty() {
			return Err(Error::Empty);
		}

		//


		let parent = match self.store.block(BlockRef::Hash(block.header().previous_header_hash.clone())) {
			Some(b) => b,
			None => { return Ok(Chain::Orphan); }
		};

		Ok(Chain::Main)
	}
}

#[cfg(test)]
mod tests {

	use super::ChainVerifier;
	use super::super::{Verify, Chain};
	use primitives::hash::H256;
	use chain;
	use std::collections::HashMap;
	use db::{self, BlockRef, Store};
	use serialization;
	use serialization::bytes::Bytes;
	use test_data;
	use std::sync::Arc;

	#[derive(Default)]
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

		fn with_blocks(blocks: &[chain::Block]) -> Self {
			let mut storage = TestStorage::default();
			let mut height = 0;
			for (idx, block) in blocks.iter().enumerate() {
				let hash = block.hash();
				storage.blocks.insert(hash.clone(), block.clone());
				storage.heights.insert(height, hash);
			}
			storage
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
			self.block(block_ref)
				.map(|b| b.transactions().iter().cloned().collect())
				.unwrap_or(Vec::new())
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

	#[test]
	fn verify_orphan() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b2 = test_data::block_h2();
		let verifier = ChainVerifier::new(Arc::new(storage));

		assert_eq!(Chain::Orphan, verifier.verify(&b2).unwrap());
	}

	#[test]
	fn verify_smoky() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b1 = test_data::block_h1();
		let verifier = ChainVerifier::new(Arc::new(storage));
		assert_eq!(Chain::Main, verifier.verify(&b1).unwrap());
	}

}
