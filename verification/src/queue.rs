//! Blocks verification queue

use chain::Block;
use primitives::hash::H256;
use super::{Chain, Verify, BlockStatus};
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use std::collections::HashSet;

pub struct VerifiedBlock {
	pub chain: Chain,
	pub block: Block,
}

impl VerifiedBlock {
	fn new(chain: Chain, block: Block) -> Self {
		VerifiedBlock { chain: chain, block: block }
	}
}

/// Verification queue
pub struct Queue {
	verifier: Box<Verify>,
	items: RwLock<LinkedHashMap<H256, Block>>,
	verified: RwLock<LinkedHashMap<H256, VerifiedBlock>>,
	invalid: RwLock<HashSet<H256>>,
}

impl Queue {

	/// New verification queue
	pub fn new(verifier: Box<Verify>) -> Self {
		Queue {
			verifier: verifier,
			items: RwLock::new(LinkedHashMap::new()),
			verified: RwLock::new(LinkedHashMap::new()),
			invalid: RwLock::new(HashSet::new()),
		}
	}

	/// Process one block in the queue
	pub fn process(&self) {
		let (hash, block) = {
			let mut items = self.items.write();
			match items.pop_front() {
				Some((hash, block)) => (hash, block),
				/// nothing to verify
				None => { return; },
			}
		};

		match self.verifier.verify(&block) {
			Ok(chain) => {
				let mut verified = self.verified.write();
				verified.insert(hash, VerifiedBlock::new(chain, block));
			},
			Err(e) => {
				println!("Verification failed: {:?}", e);
				let mut invalid = self.invalid.write();
				invalid.insert(hash);
			}
		}
	}

	/// Query block status
	pub fn block_status(&self, hash: &H256) -> BlockStatus {
		if self.invalid.read().contains(hash) { BlockStatus::Invalid }
		else if self.verified.read().contains_key(hash) { BlockStatus::Valid }
		else if self.items.read().contains_key(hash) { BlockStatus::Pending }
		else { BlockStatus::Absent }
	}
}

#[cfg(test)]
mod tests {
	use super::Queue;
	use super::super::{BlockStatus, VerificationResult, Verify, Chain};
	use chain::Block;
	use primitives::hash::H256;

	struct FacileVerifier;
	impl Verify for FacileVerifier {
		fn verify(&self, _block: &Block) -> VerificationResult { Ok(Chain::Main) }
	}

	#[test]
	fn new() {
		let queue = Queue::new(Box::new(FacileVerifier));
		assert_eq!(queue.block_status(&H256::from(0u8)), BlockStatus::Absent);
	}

}
