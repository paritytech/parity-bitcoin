//! Blocks verification queue

use chain::{Block, RepresentH256};
use primitives::hash::H256;
use super::{Chain, ContinueVerify, BlockStatus, Error as VerificationError, TransactionError};
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use std::collections::{HashSet, VecDeque};

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


pub enum ScheduleItem {
	Block(Block),
	Continued(Block, usize),
}

impl ScheduleItem {
	fn block(self) -> Block {
		match self {
			ScheduleItem::Block(block) => block,
			ScheduleItem::Continued(block, _) => block,
		}
	}
}

struct Schedule {
	items: VecDeque<(H256, ScheduleItem)>,
	set: HashSet<H256>,
}

impl Schedule {
	pub fn new() -> Self {
		Schedule { items: VecDeque::new(), set: HashSet::new() }
	}

	pub fn push_back(&mut self, hash: H256, item: ScheduleItem) {
		self.items.push_back((hash.clone(), item));
		self.set.insert(hash);
	}

	pub fn push_front(&mut self, hash: H256, item: ScheduleItem) {
		self.items.push_front((hash.clone(), item));
		self.set.insert(hash);
	}

	pub fn pop_front(&mut self) -> Option<(H256, ScheduleItem)> {
		self.items.pop_front()
			.and_then(|(h, b)| { self.set.remove(&h); Some((h, b)) })
	}

	pub fn contains(&self, hash: &H256) -> bool {
		self.set.contains(hash)
	}

	pub fn len(&self) -> usize {
		self.set.len()
	}

	pub fn front(&self) -> Option<&ScheduleItem> {
		self.items.front().and_then(|&(_, ref item)| Some(item))
	}
}

/// Verification queue
pub struct Queue {
	verifier: Box<ContinueVerify<State=usize>>,
	items: RwLock<Schedule>,
	// todo: write lock on verified should continue until blocks are persisted in the database
	verified: RwLock<LinkedHashMap<H256, VerifiedBlock>>,
	invalid: RwLock<HashSet<H256>>,
	processing: RwLock<HashSet<H256>>,
}

pub enum WorkStatus {
	Continue,
	Wait,
}

impl Queue {

	/// New verification queue
	pub fn new(verifier: Box<ContinueVerify<State=usize>>) -> Self {
		Queue {
			verifier: verifier,
			items: RwLock::new(Schedule::new()),
			verified: RwLock::new(LinkedHashMap::new()),
			invalid: RwLock::new(HashSet::new()),
			processing: RwLock::new(HashSet::new()),
		}
	}

	/// Process one block in the queue
	pub fn process(&self) -> WorkStatus {
		let (hash, item) = {
			let mut processing = self.processing.write();
			let mut items = self.items.write();

			if let Some(&ScheduleItem::Continued(_, _)) = items.front() {
				if !self.verified.read().is_empty() || !processing.is_empty() {
					// stall here until earlier blocks are processed
					return WorkStatus::Wait
				}
			}

			match items.pop_front() {
				Some((hash, item)) => {
					processing.insert(hash.clone());
					(hash, item)
				},
				/// nothing to verify
				_ => { return WorkStatus::Wait; },
			}
		};

		match {
			match item {
				ScheduleItem::Block(ref block) => self.verifier.verify(block),
				ScheduleItem::Continued(ref block, state) => self.verifier.continue_verify(block, state),
			}
		}
		{
			Ok(chain) => {
				let mut verified = self.verified.write();
				let mut processing = self.processing.write();
				processing.remove(&hash);
				verified.insert(hash, VerifiedBlock::new(chain, item.block()));
			},
			// todo: more generic incloncusive variant type for this match?
			Err(VerificationError::Transaction(num, TransactionError::Inconclusive(_))) => {
				let mut processing = self.processing.write();
				let mut items = self.items.write();
				processing.remove(&hash);
				items.push_front(hash, ScheduleItem::Continued(item.block(), num));
			},
			Err(e) => {
				println!("Verification failed: {:?}", e);
				let mut invalid = self.invalid.write();
				let mut processing = self.processing.write();

				processing.remove(&hash);
				invalid.insert(hash);
			}
		}

		WorkStatus::Continue
	}

	/// Query block status
	pub fn block_status(&self, hash: &H256) -> BlockStatus {
		if self.invalid.read().contains(hash) { BlockStatus::Invalid }
		else if self.processing.read().contains(hash) { BlockStatus::Verifying }
		else if self.verified.read().contains_key(hash) { BlockStatus::Valid }
		else if self.items.read().contains(hash) { BlockStatus::Pending }
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
		items.push_back(hash, ScheduleItem::Block(block));

		Ok(())
	}

	pub fn pop_valid(&self) -> Option<(H256, VerifiedBlock)> {
		self.verified.write().pop_front()
	}
}

#[cfg(test)]
mod tests {
	use super::Queue;
	use super::super::{BlockStatus, VerificationResult, Verify, ContinueVerify, Chain, Error as VerificationError, TransactionError};
	use chain::{Block, RepresentH256};
	use primitives::hash::H256;
	use test_data;
	use std::collections::HashMap;

	struct FacileVerifier;
	impl Verify for FacileVerifier {
		fn verify(&self, _block: &Block) -> VerificationResult { Ok(Chain::Main) }
	}

	impl ContinueVerify for FacileVerifier {
		type State = usize;
		fn continue_verify(&self, _block: &Block, _state: usize) -> VerificationResult { Ok(Chain::Main) }
	}

	struct EvilVerifier;
	impl Verify for EvilVerifier {
		fn verify(&self, _block: &Block) -> VerificationResult { Err(VerificationError::Empty) }
	}

	impl ContinueVerify for EvilVerifier {
		type State = usize;
		fn continue_verify(&self, _block: &Block, _state: usize) -> VerificationResult { Ok(Chain::Main) }
	}

	struct HupVerifier {
		hups: HashMap<H256, usize>,
	}

	impl Verify for HupVerifier {
		fn verify(&self, block: &Block) -> VerificationResult {
			if let Some(hup) = self.hups.get(&block.hash()) {
				Err(VerificationError::Transaction(*hup, TransactionError::Inconclusive(H256::from(0))))
			}
			else {
				Ok(Chain::Main)
			}
		}
	}

	impl ContinueVerify for HupVerifier {
		type State = usize;
		fn continue_verify(&self, _block: &Block, _state: usize) -> VerificationResult { Ok(Chain::Main) }
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


	#[test]
	fn verification_stalls_on_unverifiable() {
		let b1 = test_data::block_builder()
			.header().build()
			.build();
		let b2 = test_data::block_builder()
			.header().parent(b1.hash()).build()
			.build();

		let mut hup_verifier = HupVerifier { hups: HashMap::new() };
		hup_verifier.hups.insert(b2.hash(), 5);

		let queue = Queue::new(Box::new(hup_verifier));
		queue.push(b1.clone()).unwrap();
		queue.push(b2.clone()).unwrap();

		queue.process();
		assert_eq!(queue.block_status(&b1.hash()), BlockStatus::Valid);

		queue.process();
		assert_eq!(queue.block_status(&b2.hash()),
			BlockStatus::Pending,
			"Block #2 supposed to stay in the pending state, because it requires 'processing' and 'verified' lines to be empty to continue" );

	}

	#[test]
	fn verification_continues_stalled_block() {
		let b1 = test_data::block_builder()
			.header().build()
			.build();
		let b2 = test_data::block_builder()
			.header().parent(b1.hash()).build()
			.build();

		let mut hup_verifier = HupVerifier { hups: HashMap::new() };
		hup_verifier.hups.insert(b2.hash(), 5);

		let queue = Queue::new(Box::new(hup_verifier));
		queue.push(b1.clone()).unwrap();
		queue.push(b2.clone()).unwrap();

		queue.process();
		assert_eq!(queue.block_status(&b1.hash()), BlockStatus::Valid);

		queue.process();
		assert_eq!(queue.block_status(&b2.hash()),
			BlockStatus::Pending,
			"Block #2 supposed to stay in the pending state, because it requires 'processing' and 'verified' lines to be empty to continue" );

		queue.pop_valid();
		queue.process();

		assert_eq!(queue.block_status(&b2.hash()),
			BlockStatus::Valid,
			"Block #2 supposed to achieve valid state, because it requires 'processing' and 'verified' lines to be empty, which are indeed empty" );
	}

}
