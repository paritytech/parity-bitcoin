//! Blocks verification queue

use chain::{Block, BlockHeader};
use primitives::hash::H256;
use super::{Chain, Verify};
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use std::collections::HashSet;

pub struct VerifiedBlock {
	chain: Chain,
	block: Block,
}

impl VerifiedBlock {
	fn new(chain: Chain, block: Block) -> Self {
		VerifiedBlock { chain: chain, block: block }
	}
}

pub struct Queue {
	verifier: Box<Verify>,
	items: RwLock<LinkedHashMap<H256, Block>>,
	verified: RwLock<LinkedHashMap<H256, VerifiedBlock>>,
	invalid: RwLock<HashSet<H256>>,
}

impl Queue {
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
}
