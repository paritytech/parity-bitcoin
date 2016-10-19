//! Blocks verification queue

use chain::Block;
use primitives::hash::H256;
use super::{Chain, Verify, BlockStatus};
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use std::collections::HashSet;

const MAX_PENDING_PRESET: usize = 128;

pub struct VerifiedBlock {
	pub chain: Chain,
	pub block: Block,
}

impl VerifiedBlock {
	fn new(chain: Chain, block: Block) -> Self {
		VerifiedBlock { chain: chain, block: block }
	}
}

#[derive(Debug)]
/// Queue errors
pub enum Error {
	/// Queue is currently full
	Full,
	/// There is already block in the queue
	Duplicate,
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

	pub fn max_pending(&self) -> usize {
		// todo: later might be calculated with lazy-static here based on memory usage
		MAX_PENDING_PRESET
	}

	pub fn push(&self, block: Block) -> Result<(), Error> {
		let hash = block.hash();

		if self.block_status(&hash) != BlockStatus::Absent { return Err(Error::Duplicate) }

		let mut items = self.items.write();
		if items.len() > self.max_pending() { return Err(Error::Full) }
		items.insert(hash, block);

		Ok(())
	}

	pub fn pop_valid(&self) -> Option<(H256, VerifiedBlock)> {
		self.verified.write().pop_front()
	}
}

#[cfg(test)]
mod tests {
	use super::Queue;
	use super::super::{BlockStatus, VerificationResult, Verify, Chain, Error as VerificationError};
	use chain::Block;
	use primitives::hash::H256;
	use test_data;

	struct FacileVerifier;
	impl Verify for FacileVerifier {
		fn verify(&self, _block: &Block) -> VerificationResult { Ok(Chain::Main) }
	}

	struct EvilVerifier;
	impl Verify for EvilVerifier {
		fn verify(&self, _block: &Block) -> VerificationResult { Err(VerificationError::Empty) }
	}

	#[test]
	fn new() {
		let queue = Queue::new(Box::new(FacileVerifier));
		assert_eq!(queue.block_status(&H256::from(0u8)), BlockStatus::Absent);
	}

	#[test]
	fn push() {
		let queue = Queue::new(Box::new(FacileVerifier));
		let block = test_data::block1();
		let hash = block.hash();

		queue.push(block).unwrap();

		assert_eq!(queue.block_status(&hash), BlockStatus::Pending);
	}

	#[test]
	fn push_duplicate() {
		let queue = Queue::new(Box::new(FacileVerifier));
		let block = test_data::block1();
		let dup_block = test_data::block1();

		queue.push(block).unwrap();
		let second_push = queue.push(dup_block);

		assert!(second_push.is_err());
	}

	#[test]
	fn process_happy() {
		let queue = Queue::new(Box::new(FacileVerifier));
		let block = test_data::block1();
		let hash = block.hash();

		queue.push(block).unwrap();
		queue.process();

		assert_eq!(queue.block_status(&hash), BlockStatus::Valid);
	}

	#[test]
	fn process_unhappy() {
		let queue = Queue::new(Box::new(EvilVerifier));
		let block = test_data::block1();
		let hash = block.hash();

		queue.push(block).unwrap();
		queue.process();

		assert_eq!(queue.block_status(&hash), BlockStatus::Invalid);
	}

	#[test]
	fn process_async() {
		use std::thread;
		use std::sync::Arc;

		let queue = Arc::new(Queue::new(Box::new(FacileVerifier)));

		let t1_queue = queue.clone();
		let t1_handle = thread::spawn(move || {
			let block_h1 = test_data::block_h1();
			t1_queue.push(block_h1).unwrap();
			t1_queue.process();
		});

		let t2_queue = queue.clone();
		let t2_handle = thread::spawn(move || {
			let block_h2 = test_data::block_h2();
			t2_queue.push(block_h2).unwrap();
			t2_queue.process();
		});

		t1_handle.join().unwrap();
		t2_handle.join().unwrap();

		assert_eq!(queue.block_status(&test_data::block_h1().hash()), BlockStatus::Valid);
		assert_eq!(queue.block_status(&test_data::block_h2().hash()), BlockStatus::Valid);
	}

	#[test]
	fn pop() {
		let queue = Queue::new(Box::new(FacileVerifier));
		let block = test_data::block1();
		let hash = block.hash();

		queue.push(block).unwrap();
		queue.process();
		let (h, _b) = queue.pop_valid().unwrap();

		assert_eq!(queue.block_status(&hash), BlockStatus::Absent);
		assert_eq!(h, hash);
	}
}
