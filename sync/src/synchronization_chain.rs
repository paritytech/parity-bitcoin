use std::fmt;
use std::sync::Arc;
use parking_lot::RwLock;
use chain::Block;
use db;
use primitives::hash::H256;
use hash_queue::{HashQueueChain, HashPosition};
use miner::MemoryPool;

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

/// Result of intersecting chain && inventory
#[derive(Debug, PartialEq)]
pub enum InventoryIntersection {
	/// 3.2: No intersection with in-memory queue && no intersection with db
	NoKnownBlocks(usize),
	/// 2.3: Inventory has no new blocks && some of blocks in inventory are in in-memory queue
	InMemoryNoNewBlocks,
	/// 2.4.2: Inventory has new blocks && these blocks are right after chain' best block
	InMemoryMainNewBlocks(usize),
	/// 2.4.3: Inventory has new blocks && these blocks are forked from our chain' best block
	InMemoryForkNewBlocks(usize),
	/// 3.3: No intersection with in-memory queue && has intersection with db && all blocks are already stored in db
	DbAllBlocksKnown,
	/// 3.4: No intersection with in-memory queue && has intersection with db && some blocks are not yet stored in db
	DbForkNewBlocks(usize),
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
	/// Transactions memory pool
	memory_pool: MemoryPool,
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
		match *self {
			BlockState::Scheduled => SCHEDULED_QUEUE,
			BlockState::Requested => REQUESTED_QUEUE,
			BlockState::Verifying => VERIFYING_QUEUE,
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
			memory_pool: MemoryPool::new(),
		}
	}

	/// Get information on current blockchain state
	pub fn information(&self) -> Information {
		Information {
			scheduled: self.hash_chain.len_of(SCHEDULED_QUEUE),
			requested: self.hash_chain.len_of(REQUESTED_QUEUE),
			verifying: self.hash_chain.len_of(VERIFYING_QUEUE),
			stored: self.best_storage_block.number + 1,
		}
	}

	/// Get storage
	pub fn storage(&self) -> Arc<db::Store> {
		self.storage.clone()
	}

	/// Get memory pool reference
	pub fn memory_pool(&self) -> &MemoryPool {
		&self.memory_pool
	}

	/// Get mutable memory pool reference
	#[cfg(test)]
	pub fn memory_pool_mut<'a>(&'a mut self) -> &'a mut MemoryPool {
		&mut self.memory_pool
	}

	/// Get number of blocks in given state
	pub fn length_of_state(&self, state: BlockState) -> u32 {
		match state {
			BlockState::Stored => self.best_storage_block.number + 1,
			_ => self.hash_chain.len_of(state.to_queue_index()),
		}
	}

	/// Get best block
	pub fn best_block(&self) -> db::BestBlock {
		match self.hash_chain.back() {
			Some(hash) => db::BestBlock {
				number: self.best_storage_block.number + self.hash_chain.len(),
				hash: hash.clone(),
			},
			None => self.best_storage_block.clone(),
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

	/// Prepare block locator hashes, as described in protocol documentation:
	/// https://en.bitcoin.it/wiki/Protocol_documentation#getblocks
	/// When there are forked blocks in the queue, this method can result in
	/// mixed block locator hashes ([0 - from fork1, 1 - from fork2, 2 - from fork1]).
	/// Peer will respond with blocks of fork1 || fork2 => we could end up in some side fork
	/// To resolve this, after switching to saturated state, we will also ask all peers for inventory.
	pub fn block_locator_hashes(&self) -> Vec<H256> {
		let mut block_locator_hashes: Vec<H256> = Vec::new();

		// calculate for hash_queue
		let (local_index, step) = self.block_locator_hashes_for_queue(&mut block_locator_hashes);

		// calculate for storage
		let storage_index = if self.best_storage_block.number < local_index { 0 } else { self.best_storage_block.number - local_index };
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
		// insert to storage
		try!(self.storage.insert_block(&block));

		// remember new best block hash
		self.best_storage_block = self.storage.best_block().expect("Inserted block above");

		Ok(())
	}

	/// Remove block
	pub fn remove_block(&mut self, hash: &H256) {
		if self.hash_chain.remove_at(SCHEDULED_QUEUE, hash) == HashPosition::Missing
			&& self.hash_chain.remove_at(REQUESTED_QUEUE, hash) == HashPosition::Missing {
			self.hash_chain.remove_at(VERIFYING_QUEUE, hash);
		}
	}

	/// Remove block by hash if it is currently in given state
	pub fn remove_block_with_state(&mut self, hash: &H256, state: BlockState) -> HashPosition {
		self.hash_chain.remove_at(state.to_queue_index(), hash)
	}

	/// Remove all blocks with given state
	pub fn remove_blocks_with_state(&mut self, state: BlockState) {
		self.hash_chain.remove_all_at(state.to_queue_index());
	}

	/// Intersect chain with inventory
	pub fn intersect_with_inventory(&self, inventory: &[H256]) -> InventoryIntersection {
		let inventory_len = inventory.len();
		assert!(inventory_len != 0);

		// giving that blocks in inventory are ordered
		match self.block_state(&inventory[0]) {
			// if first block of inventory is unknown => all other blocks are also unknown
			BlockState::Unknown => {
				InventoryIntersection::NoKnownBlocks(0)
			},
			// else if first block is known
			first_block_state => match self.block_state(&inventory[inventory_len - 1]) {
				// if last block is known to be in db => all inventory blocks are also in db
				BlockState::Stored => {
					InventoryIntersection::DbAllBlocksKnown
				},
				// if first block is known && last block is unknown => intersection with queue or with db
				BlockState::Unknown => {
					// find last known block
					let mut previous_state = first_block_state;
					for (index, item) in inventory.iter().enumerate().take(inventory_len).skip(1) {
						let state = self.block_state(item);
						if state == BlockState::Unknown {
							// previous block is stored => fork from stored block
							if previous_state == BlockState::Stored {
								return InventoryIntersection::DbForkNewBlocks(index);
							}
							// previous block is best block => no fork
							else if &self.best_block().hash == &inventory[index - 1] {
								return InventoryIntersection::InMemoryMainNewBlocks(index);
							}
							// previous block is not a best block => fork
							else {
								return InventoryIntersection::InMemoryForkNewBlocks(index);
							}
						}
						previous_state = state;
					}

					// unreachable because last block is unknown && in above loop we search for unknown blocks
					unreachable!();
				},
				// if first block is known && last block is also known && is in queue => queue intersection with no new block
				_ => {
					InventoryIntersection::InMemoryNoNewBlocks
				}
			}
		}
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
			let mut num = self.best_storage_block.number;
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
	use chain::RepresentH256;
	use hash_queue::HashPosition;
	use super::{Chain, BlockState, InventoryIntersection};
	use db::{self, Store, BestBlock};
	use primitives::hash::H256;
	use test_data;

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
		assert_eq!(&chain.best_block(), &db_best_block);
		assert_eq!(chain.block_state(&db_best_block.hash), BlockState::Stored);
		assert_eq!(chain.block_state(&H256::from(0)), BlockState::Unknown);
	}

	#[test]
	fn chain_block_path() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut chain = Chain::new(db.clone());

		// add 6 blocks to scheduled queue
		chain.schedule_blocks_hashes(vec![
			H256::from(0),
			H256::from(1),
			H256::from(2),
			H256::from(3),
			H256::from(4),
			H256::from(5),
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
		assert_eq!(chain.remove_block_with_state(&H256::from(0), BlockState::Scheduled), HashPosition::Missing);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 3
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// remove blocks 0 & 1 from requested queue
		assert_eq!(chain.remove_block_with_state(&H256::from(1), BlockState::Requested), HashPosition::Inside);
		assert_eq!(chain.remove_block_with_state(&H256::from(0), BlockState::Requested), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// mark 0 & 1 as verifying
		chain.verify_block_hash(H256::from(1));
		chain.verify_block_hash(H256::from(2));
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 2 && chain.information().stored == 1);

		// mark block 0 as verified
		assert_eq!(chain.remove_block_with_state(&H256::from(1), BlockState::Verifying), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 1);
		// insert new best block to the chain
		chain.insert_best_block(test_data::block_h1()).expect("Db error");
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 2);
		assert_eq!(db.best_block().expect("storage with genesis block is required").number, 1);
	}

	#[test]
	fn chain_block_locator_hashes() {
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		let genesis_hash = chain.best_block().hash;
		assert_eq!(chain.block_locator_hashes(), vec![genesis_hash.clone()]);

		let block1 = test_data::block_h1();
		let block1_hash = block1.hash();

		chain.insert_best_block(block1).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block1_hash.clone(), genesis_hash.clone()]);

		let block2 = test_data::block_h2();
		let block2_hash = block2.hash();

		chain.insert_best_block(block2).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block2_hash.clone(), block1_hash.clone(), genesis_hash.clone()]);

		chain.schedule_blocks_hashes(vec![
			H256::from(0),
			H256::from(1),
			H256::from(2),
			H256::from(3),
			H256::from(4),
			H256::from(5),
			H256::from(6),
			H256::from(7),
			H256::from(8),
			H256::from(9),
			H256::from(10),
		]);
		chain.request_blocks_hashes(10);
		chain.verify_blocks_hashes(10);

		assert_eq!(chain.block_locator_hashes(), vec![
			H256::from(10),
			H256::from(9),
			H256::from(8),
			H256::from(7),
			H256::from(6),
			H256::from(5),
			H256::from(4),
			H256::from(3),
			H256::from(2),
			H256::from(1),
			block2_hash.clone(),
			genesis_hash.clone(),
		]);

		chain.schedule_blocks_hashes(vec![
			H256::from(11),
			H256::from(12),
			H256::from(13),
			H256::from(14),
			H256::from(15),
			H256::from(16),
		]);
		chain.request_blocks_hashes(10);

		assert_eq!(chain.block_locator_hashes(), vec![
			H256::from(16),
			H256::from(15),
			H256::from(14),
			H256::from(13),
			H256::from(12),
			H256::from(11),
			H256::from(10),
			H256::from(9),
			H256::from(8),
			H256::from(7),
			H256::from(5),
			H256::from(1),
			genesis_hash.clone(),
		]);

		chain.schedule_blocks_hashes(vec![
			H256::from(20),
			H256::from(21),
			H256::from(22),
		]);

		assert_eq!(chain.block_locator_hashes(), vec![
			H256::from(22),
			H256::from(21),
			H256::from(20),
			H256::from(16),
			H256::from(15),
			H256::from(14),
			H256::from(13),
			H256::from(12),
			H256::from(11),
			H256::from(10),
			H256::from(8),
			H256::from(4),
			genesis_hash.clone(),
		]);
	}

	#[test]
	fn chain_intersect_with_inventory() {
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		// append 2 db blocks
		chain.insert_best_block(test_data::block_h1()).expect("Error inserting new block");
		chain.insert_best_block(test_data::block_h2()).expect("Error inserting new block");
		// append 3 verifying blocks
		chain.schedule_blocks_hashes(vec![
			H256::from(0),
			H256::from(1),
			H256::from(2),
		]);
		chain.request_blocks_hashes(3);
		chain.verify_blocks_hashes(3);
		// append 3 requested blocks
		chain.schedule_blocks_hashes(vec![
			H256::from(10),
			H256::from(11),
			H256::from(12),
		]);
		chain.request_blocks_hashes(10);
		// append 3 scheduled blocks
		chain.schedule_blocks_hashes(vec![
			H256::from(20),
			H256::from(21),
			H256::from(22),
		]);

		assert_eq!(chain.intersect_with_inventory(&vec![
			H256::from(30),
			H256::from(31),
		]), InventoryIntersection::NoKnownBlocks(0));

		assert_eq!(chain.intersect_with_inventory(&vec![
			H256::from(2),
			H256::from(10),
			H256::from(11),
			H256::from(12),
			H256::from(20),
		]), InventoryIntersection::InMemoryNoNewBlocks);

		assert_eq!(chain.intersect_with_inventory(&vec![
			H256::from(21),
			H256::from(22),
			H256::from(30),
			H256::from(31),
		]), InventoryIntersection::InMemoryMainNewBlocks(2));

		assert_eq!(chain.intersect_with_inventory(&vec![
			H256::from(20),
			H256::from(21),
			H256::from(30),
			H256::from(31),
		]), InventoryIntersection::InMemoryForkNewBlocks(2));

		assert_eq!(chain.intersect_with_inventory(&vec![
			test_data::block_h1().hash(),
			test_data::block_h2().hash(),
		]), InventoryIntersection::DbAllBlocksKnown);

		assert_eq!(chain.intersect_with_inventory(&vec![
			test_data::block_h2().hash(),
			H256::from(30),
			H256::from(31),
		]), InventoryIntersection::DbForkNewBlocks(1));
	}
}
