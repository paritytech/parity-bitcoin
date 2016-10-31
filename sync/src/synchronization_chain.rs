use std::fmt;
use std::sync::Arc;
use parking_lot::RwLock;
use chain::{Block, RepresentH256};
use db;
use primitives::hash::H256;
use hash_queue::{HashQueueChain, HashPosition};

/// Thread-safe reference to `Chain`
pub type ChainRef = Arc<RwLock<Chain>>;

/// Index of 'verifying' queue
const VERIFYING_QUEUE: usize = 0;
/// Index of 'requested' queue
const REQUESTED_QUEUE: usize = 1;
/// Index of 'scheduled' queue
const SCHEDULED_QUEUE: usize = 2;
/// Number of hash queues
const NUMBER_OF_QUEUES: usize = 3;

/// Block synchronization state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BlockState {
	/// Block is unknown
	Unknown,
	/// Scheduled for requesting
	Scheduled,
	/// Requested from peers
	Requested,
	/// Currently verifying
	Verifying,
	/// In storage
	Stored,
}

/// Synchronization chain information
#[derive(Debug)]
pub struct Information {
	/// Number of blocks currently scheduled for requesting
	pub scheduled: u32,
	/// Number of blocks currently requested from peers
	pub requested: u32,
	/// Number of blocks currently verifying
	pub verifying: u32,
	/// Number of blocks in the storage
	pub stored: u32,
}

/// Blockchain from synchroniation point of view, consisting of:
/// 1) all blocks from the `storage` [oldest blocks]
/// 2) all blocks currently verifying by `verification_queue`
/// 3) all blocks currently requested from peers
/// 4) all blocks currently scheduled for requesting [newest blocks]
pub struct Chain {
	/// Genesis block hash (stored for optimizations)
	genesis_block_hash: H256,
	/// Best storage block (stored for optimizations)
	best_storage_block: db::BestBlock,
	/// Local blocks storage
	storage: Arc<db::Store>,
	/// In-memory queue of blocks hashes
	hash_chain: HashQueueChain,
}

impl BlockState {
	pub fn from_queue_index(queue_index: usize) -> BlockState {
		match queue_index {
			SCHEDULED_QUEUE => BlockState::Scheduled,
			REQUESTED_QUEUE => BlockState::Requested,
			VERIFYING_QUEUE => BlockState::Verifying,
			_ => panic!("Unsupported queue_index: {}", queue_index),
		}
	}

	pub fn to_queue_index(&self) -> usize {
		match self {
			&BlockState::Scheduled => SCHEDULED_QUEUE,
			&BlockState::Requested => REQUESTED_QUEUE,
			&BlockState::Verifying => VERIFYING_QUEUE,
			_ => panic!("Unsupported queue: {:?}", self),
		}
	}
}

impl Chain {
	/// Create new `Chain` with given storage
	pub fn new(storage: Arc<db::Store>) -> Self {
		// we only work with storages with genesis block
		let genesis_block_hash = storage.block_hash(0)
			.expect("storage with genesis block is required");
		let best_storage_block = storage.best_block()
			.expect("non-empty storage is required");

		Chain {
			genesis_block_hash: genesis_block_hash,
			best_storage_block: best_storage_block,
			storage: storage,
			hash_chain: HashQueueChain::with_number_of_queues(NUMBER_OF_QUEUES),
		}
	}

	/// Get information on current blockchain state
	pub fn information(&self) -> Information {
		Information {
			scheduled: self.hash_chain.len_of(SCHEDULED_QUEUE),
			requested: self.hash_chain.len_of(REQUESTED_QUEUE),
			verifying: self.hash_chain.len_of(VERIFYING_QUEUE),
			stored: self.storage.best_block().map_or(0, |block| block.number + 1),
		}
	}

	/// Get storage
	pub fn storage(&self) -> Arc<db::Store> {
		self.storage.clone()
	}

	/// Get number of blocks in given state
	pub fn length_of_state(&self, state: BlockState) -> u32 {
		match state {
			BlockState::Stored => self.best_storage_block.number + 1,
			_ => self.hash_chain.len_of(state.to_queue_index()),
		}
	}

	/// Returns true if has blocks of given type
	pub fn has_blocks_of_state(&self, state: BlockState) -> bool {
		match state {
			BlockState::Stored => true, // storage with genesis block is required
			_ => !self.hash_chain.is_empty_at(state.to_queue_index()),
		}
	}

	/// Get best block
	pub fn best_block(&self) -> db::BestBlock {
		let storage_best_block = self.storage.best_block().expect("storage with genesis block is required");
		match self.hash_chain.back() {
			Some(hash) => db::BestBlock {
				number: storage_best_block.number + self.hash_chain.len(),
				hash: hash.clone(),
			},
			None => db::BestBlock {
				number: storage_best_block.number,
				hash: storage_best_block.hash,
			}
		}
	}

	/// Get best block of given state
	pub fn best_block_of_state(&self, state: BlockState) -> Option<db::BestBlock> {
		match state {
			BlockState::Scheduled => self.hash_chain.back_at(SCHEDULED_QUEUE)
				.map(|hash| db::BestBlock {
					hash: hash,
					number: self.storage.best_block().expect("storage with genesis block is required").number + 1
						+ self.hash_chain.len_of(VERIFYING_QUEUE)
						+ self.hash_chain.len_of(REQUESTED_QUEUE)
						+ self.hash_chain.len_of(SCHEDULED_QUEUE)
				}),
			BlockState::Requested => self.hash_chain.back_at(REQUESTED_QUEUE)
				.map(|hash| db::BestBlock {
					hash: hash,
					number: self.storage.best_block().expect("storage with genesis block is required").number + 1
						+ self.hash_chain.len_of(VERIFYING_QUEUE)
						+ self.hash_chain.len_of(REQUESTED_QUEUE)
				}),
			BlockState::Verifying => self.hash_chain.back_at(VERIFYING_QUEUE)
				.map(|hash| db::BestBlock {
					hash: hash,
					number: self.storage.best_block().expect("storage with genesis block is required").number + 1
						+ self.hash_chain.len_of(VERIFYING_QUEUE)
				}),
			BlockState::Stored => {
					self.storage.best_block()
				},
			_ => panic!("not supported"),
		}
	}

	/// Check if block has given state
	pub fn block_has_state(&self, hash: &H256, state: BlockState) -> bool {
		match state {
			BlockState::Scheduled => self.hash_chain.is_contained_in(SCHEDULED_QUEUE, hash),
			BlockState::Requested => self.hash_chain.is_contained_in(REQUESTED_QUEUE, hash),
			BlockState::Verifying => self.hash_chain.is_contained_in(VERIFYING_QUEUE, hash),
			BlockState::Stored => self.storage.contains_block(db::BlockRef::Hash(hash.clone())),
			BlockState::Unknown => self.block_state(hash) == BlockState::Unknown,
		}
	}

	/// Get block state
	pub fn block_state(&self, hash: &H256) -> BlockState {
		match self.hash_chain.contains_in(hash) {
			Some(queue_index) => BlockState::from_queue_index(queue_index),
			None => if self.storage.contains_block(db::BlockRef::Hash(hash.clone())) {
				BlockState::Stored
			} else {
				BlockState::Unknown
			},
		}
	}

	/// Prepare best block locator hashes
	pub fn best_block_locator_hashes(&self) -> Vec<H256> {
		let mut result: Vec<H256> = Vec::with_capacity(4);
		if let Some(pre_best_block) = self.hash_chain.back_skip_n_at(SCHEDULED_QUEUE, 2) {
			result.push(pre_best_block);
		}
		if let Some(pre_best_block) = self.hash_chain.back_skip_n_at(REQUESTED_QUEUE, 2) {
			result.push(pre_best_block);
		}
		if let Some(pre_best_block) = self.hash_chain.back_skip_n_at(VERIFYING_QUEUE, 2) {
			result.push(pre_best_block);
		}
		result.push(self.best_storage_block.hash.clone());
		result
	}

	/// Prepare block locator hashes, as described in protocol documentation:
	/// https://en.bitcoin.it/wiki/Protocol_documentation#getblocks
	pub fn block_locator_hashes(&self) -> Vec<H256> {
		let mut block_locator_hashes: Vec<H256> = Vec::new();

		// calculate for hash_queue
		let (local_index, step) = self.block_locator_hashes_for_queue(&mut block_locator_hashes);

		// calculate for storage
		let storage_best_block_number = self.storage.best_block().expect("storage with genesis block is required").number;
		let storage_index = if storage_best_block_number < local_index { 0 } else { storage_best_block_number - local_index };
		self.block_locator_hashes_for_storage(storage_index, step, &mut block_locator_hashes);
		block_locator_hashes
	}

	/// Schedule blocks for requesting
	pub fn schedule_blocks_hashes(&mut self, hashes: Vec<H256>) {
		self.hash_chain.push_back_n_at(SCHEDULED_QUEUE, hashes)
	}

	/// Moves n blocks from scheduled queue to requested queue
	pub fn request_blocks_hashes(&mut self, n: u32) -> Vec<H256> {
		let scheduled = self.hash_chain.pop_front_n_at(SCHEDULED_QUEUE, n);
		self.hash_chain.push_back_n_at(REQUESTED_QUEUE, scheduled.clone());
		scheduled
	}

	/// Add block to verifying queue
	pub fn verify_block_hash(&mut self, hash: H256) {
		self.hash_chain.push_back_at(VERIFYING_QUEUE, hash);
	}

	/// Moves n blocks from requested queue to verifying queue
	#[cfg(test)]
	pub fn verify_blocks_hashes(&mut self, n: u32) -> Vec<H256> {
		let requested = self.hash_chain.pop_front_n_at(REQUESTED_QUEUE, n);
		self.hash_chain.push_back_n_at(VERIFYING_QUEUE, requested.clone());
		requested
	}

	/// Insert new best block to storage
	pub fn insert_best_block(&mut self, block: Block) -> Result<(), db::Error> {
		if block.block_header.previous_header_hash != self.best_storage_block.hash {
			return Err(db::Error::DB("Trying to insert out-of-order block".into()));
		}

		// remember new best block hash
		self.best_storage_block.number += 1;
		self.best_storage_block.hash = block.hash();

		// insert to storage
		self.storage.insert_block(&block)
	}

	/// Remove block by hash if it is currently in given state
	pub fn remove_block_with_state(&mut self, hash: &H256, state: BlockState) -> HashPosition {
		self.hash_chain.remove_at(state.to_queue_index(), hash)
	}

	/// Remove all blocks with given state
	pub fn remove_blocks_with_state(&mut self, state: BlockState) {
		self.hash_chain.remove_all_at(state.to_queue_index());
	}

	/// Calculate block locator hashes for hash queue
	fn block_locator_hashes_for_queue(&self, hashes: &mut Vec<H256>) -> (u32, u32) {
		let queue_len = self.hash_chain.len();
		if queue_len == 0 {
			return (0, 1);
		}

		let mut index = queue_len - 1;
		let mut step = 1u32;
		loop {
			let block_hash = self.hash_chain[index].clone();
			hashes.push(block_hash);

			if hashes.len() >= 10 {
				step <<= 1;
			}
			if index < step {
				return (step - index - 1, step);
			}
			index -= step;
		}
	}

	/// Calculate block locator hashes for storage
	fn block_locator_hashes_for_storage(&self, mut index: u32, mut step: u32, hashes: &mut Vec<H256>) {
		loop {
			let block_hash = self.storage.block_hash(index)
				.expect("private function; index calculated in `block_locator_hashes`; qed");
			hashes.push(block_hash);

			if hashes.len() >= 10 {
				step <<= 1;
			}
			if index < step {
				// always include genesis hash
				if index != 0 {
					hashes.push(self.genesis_block_hash.clone())
				}

				break;
			}
			index -= step;
		}
	}
}

impl fmt::Debug for Chain {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "chain: ["));
		{
			let mut num = self.storage.best_block().expect("Storage with genesis block is required").number;
			try!(writeln!(f, "\tworse(stored): {} {:?}", 0, self.storage.block_hash(0)));
			try!(writeln!(f, "\tbest(stored): {} {:?}", num, self.storage.block_hash(num)));

			let queues = vec![
				("verifying", VERIFYING_QUEUE),
				("requested", REQUESTED_QUEUE),
				("scheduled", SCHEDULED_QUEUE),
			];
			for (state, queue) in queues {
				let queue_len = self.hash_chain.len_of(queue);
				if queue_len != 0 {
					try!(writeln!(f, "\tworse({}): {} {:?}", state, num + 1, self.hash_chain.front_at(queue)));
					num += queue_len;
					if let Some(pre_best) = self.hash_chain.pre_back_at(queue) {
						try!(writeln!(f, "\tpre-best({}): {} {:?}", state, num - 1, pre_best));
					}
					try!(writeln!(f, "\tbest({}): {} {:?}", state, num, self.hash_chain.back_at(queue)));
				}
			}
		}
		writeln!(f, "]")
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use chain::{Block, RepresentH256};
	use hash_queue::HashPosition;
	use super::{Chain, BlockState};
	use db::{self, Store, BestBlock};

	#[test]
	fn chain_empty() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let db_best_block = BestBlock { number: 0, hash: db.best_block().expect("storage with genesis block is required").hash };
		let chain = Chain::new(db.clone());
		assert_eq!(chain.information().scheduled, 0);
		assert_eq!(chain.information().requested, 0);
		assert_eq!(chain.information().verifying, 0);
		assert_eq!(chain.information().stored, 1);
		assert_eq!(chain.length_of_state(BlockState::Scheduled), 0);
		assert_eq!(chain.length_of_state(BlockState::Requested), 0);
		assert_eq!(chain.length_of_state(BlockState::Verifying), 0);
		assert_eq!(chain.length_of_state(BlockState::Stored), 1);
		assert_eq!(chain.has_blocks_of_state(BlockState::Scheduled), false);
		assert_eq!(chain.has_blocks_of_state(BlockState::Requested), false);
		assert_eq!(chain.has_blocks_of_state(BlockState::Verifying), false);
		assert_eq!(chain.has_blocks_of_state(BlockState::Stored), true);
		assert_eq!(&chain.best_block(), &db_best_block);
		assert_eq!(chain.best_block_of_state(BlockState::Scheduled), None);
		assert_eq!(chain.best_block_of_state(BlockState::Requested), None);
		assert_eq!(chain.best_block_of_state(BlockState::Verifying), None);
		assert_eq!(chain.best_block_of_state(BlockState::Stored), Some(db_best_block.clone()));
		assert_eq!(chain.block_has_state(&db_best_block.hash, BlockState::Requested), false);
		assert_eq!(chain.block_has_state(&db_best_block.hash, BlockState::Stored), true);
		assert_eq!(chain.block_state(&db_best_block.hash), BlockState::Stored);
		assert_eq!(chain.block_state(&"0000000000000000000000000000000000000000000000000000000000000000".into()), BlockState::Unknown);
	}

	#[test]
	fn chain_block_path() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut chain = Chain::new(db.clone());

		// add 6 blocks to scheduled queue
		chain.schedule_blocks_hashes(vec![
			"0000000000000000000000000000000000000000000000000000000000000000".into(),
			"0000000000000000000000000000000000000000000000000000000000000001".into(),
			"0000000000000000000000000000000000000000000000000000000000000002".into(),
			"0000000000000000000000000000000000000000000000000000000000000003".into(),
			"0000000000000000000000000000000000000000000000000000000000000004".into(),
			"0000000000000000000000000000000000000000000000000000000000000005".into(),
		]);
		assert!(chain.information().scheduled == 6 && chain.information().requested == 0
			&& chain.information().verifying == 0 && chain.information().stored == 1);

		// move 2 best blocks (0 && 1) to requested queue
		chain.request_blocks_hashes(2);
		assert!(chain.information().scheduled == 4 && chain.information().requested == 2
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// move 0 best blocks to requested queue
		chain.request_blocks_hashes(0);
		assert!(chain.information().scheduled == 4 && chain.information().requested == 2
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// move 1 best blocks (2) to requested queue
		chain.request_blocks_hashes(1);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 3
			&& chain.information().verifying == 0 && chain.information().stored == 1);

		// try to remove block 0 from scheduled queue => missing
		assert_eq!(chain.remove_block_with_state(&"0000000000000000000000000000000000000000000000000000000000000000".into(), BlockState::Scheduled), HashPosition::Missing);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 3
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// remove blocks 0 & 1 from requested queue
		assert_eq!(chain.remove_block_with_state(&"0000000000000000000000000000000000000000000000000000000000000001".into(), BlockState::Requested), HashPosition::Inside);
		assert_eq!(chain.remove_block_with_state(&"0000000000000000000000000000000000000000000000000000000000000000".into(), BlockState::Requested), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// mark 0 & 1 as verifying
		chain.verify_block_hash("0000000000000000000000000000000000000000000000000000000000000001".into());
		chain.verify_block_hash("0000000000000000000000000000000000000000000000000000000000000002".into());
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 2 && chain.information().stored == 1);

		// mark block 0 as verified
		assert_eq!(chain.remove_block_with_state(&"0000000000000000000000000000000000000000000000000000000000000001".into(), BlockState::Verifying), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 1);
		// insert new best block to the chain
		let block1: Block = "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e362990101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000".into();
		chain.insert_best_block(block1).expect("Db error");
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 2);
		assert_eq!(db.best_block().expect("storage with genesis block is required").number, 1);
	}

	#[test]
	fn chain_block_locator_hashes() {
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		let genesis_hash = chain.best_block().hash;
		assert_eq!(chain.block_locator_hashes(), vec![genesis_hash.clone()]);

		let block1: Block = "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e362990101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000".into();
		let block1_hash = block1.hash();

		chain.insert_best_block(block1).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block1_hash.clone(), genesis_hash.clone()]);

		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();
		let block2_hash = block2.hash();

		chain.insert_best_block(block2).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block2_hash.clone(), block1_hash.clone(), genesis_hash.clone()]);

		chain.schedule_blocks_hashes(vec![
			"0000000000000000000000000000000000000000000000000000000000000000".into(),
			"0000000000000000000000000000000000000000000000000000000000000001".into(),
			"0000000000000000000000000000000000000000000000000000000000000002".into(),
			"0000000000000000000000000000000000000000000000000000000000000003".into(),
			"0000000000000000000000000000000000000000000000000000000000000004".into(),
			"0000000000000000000000000000000000000000000000000000000000000005".into(),
			"0000000000000000000000000000000000000000000000000000000000000006".into(),
			"0000000000000000000000000000000000000000000000000000000000000007".into(),
			"0000000000000000000000000000000000000000000000000000000000000008".into(),
			"0000000000000000000000000000000000000000000000000000000000000009".into(),
			"0000000000000000000000000000000000000000000000000000000000000010".into(),
		]);
		chain.request_blocks_hashes(10);
		chain.verify_blocks_hashes(10);

		assert_eq!(chain.best_block_locator_hashes()[0], "0000000000000000000000000000000000000000000000000000000000000010".into());
		assert_eq!(chain.block_locator_hashes(), vec![
			"0000000000000000000000000000000000000000000000000000000000000010".into(),
			"0000000000000000000000000000000000000000000000000000000000000009".into(),
			"0000000000000000000000000000000000000000000000000000000000000008".into(),
			"0000000000000000000000000000000000000000000000000000000000000007".into(),
			"0000000000000000000000000000000000000000000000000000000000000006".into(),
			"0000000000000000000000000000000000000000000000000000000000000005".into(),
			"0000000000000000000000000000000000000000000000000000000000000004".into(),
			"0000000000000000000000000000000000000000000000000000000000000003".into(),
			"0000000000000000000000000000000000000000000000000000000000000002".into(),
			"0000000000000000000000000000000000000000000000000000000000000001".into(),
			block2_hash.clone(),
			genesis_hash.clone(),
		]);

		chain.schedule_blocks_hashes(vec![
			"0000000000000000000000000000000000000000000000000000000000000011".into(),
			"0000000000000000000000000000000000000000000000000000000000000012".into(),
			"0000000000000000000000000000000000000000000000000000000000000013".into(),
			"0000000000000000000000000000000000000000000000000000000000000014".into(),
			"0000000000000000000000000000000000000000000000000000000000000015".into(),
			"0000000000000000000000000000000000000000000000000000000000000016".into(),
		]);
		chain.request_blocks_hashes(10);

		assert_eq!(chain.best_block_locator_hashes()[0], "0000000000000000000000000000000000000000000000000000000000000014".into());
		assert_eq!(chain.block_locator_hashes(), vec![
			"0000000000000000000000000000000000000000000000000000000000000016".into(),
			"0000000000000000000000000000000000000000000000000000000000000015".into(),
			"0000000000000000000000000000000000000000000000000000000000000014".into(),
			"0000000000000000000000000000000000000000000000000000000000000013".into(),
			"0000000000000000000000000000000000000000000000000000000000000012".into(),
			"0000000000000000000000000000000000000000000000000000000000000011".into(),
			"0000000000000000000000000000000000000000000000000000000000000010".into(),
			"0000000000000000000000000000000000000000000000000000000000000009".into(),
			"0000000000000000000000000000000000000000000000000000000000000008".into(),
			"0000000000000000000000000000000000000000000000000000000000000007".into(),
			"0000000000000000000000000000000000000000000000000000000000000005".into(),
			"0000000000000000000000000000000000000000000000000000000000000001".into(),
			genesis_hash.clone(),
		]);

		chain.schedule_blocks_hashes(vec![
			"0000000000000000000000000000000000000000000000000000000000000020".into(),
			"0000000000000000000000000000000000000000000000000000000000000021".into(),
			"0000000000000000000000000000000000000000000000000000000000000022".into(),
		]);

		assert_eq!(chain.best_block_locator_hashes()[0], "0000000000000000000000000000000000000000000000000000000000000020".into());
		assert_eq!(chain.block_locator_hashes(), vec![
			"0000000000000000000000000000000000000000000000000000000000000022".into(),
			"0000000000000000000000000000000000000000000000000000000000000021".into(),
			"0000000000000000000000000000000000000000000000000000000000000020".into(),
			"0000000000000000000000000000000000000000000000000000000000000016".into(),
			"0000000000000000000000000000000000000000000000000000000000000015".into(),
			"0000000000000000000000000000000000000000000000000000000000000014".into(),
			"0000000000000000000000000000000000000000000000000000000000000013".into(),
			"0000000000000000000000000000000000000000000000000000000000000012".into(),
			"0000000000000000000000000000000000000000000000000000000000000011".into(),
			"0000000000000000000000000000000000000000000000000000000000000010".into(),
			"0000000000000000000000000000000000000000000000000000000000000008".into(),
			"0000000000000000000000000000000000000000000000000000000000000004".into(),
			genesis_hash.clone(),
		]);
	}
}
