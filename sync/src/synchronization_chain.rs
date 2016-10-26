use std::sync::Arc;
use parking_lot::RwLock;
use chain::Block;
use db;
use primitives::hash::H256;
use best_block::BestBlock;
use hash_queue::{HashQueue, HashQueueChain, HashPosition};
use verification;

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
	pub scheduled: u64,
	/// Number of blocks currently requested from peers
	pub requested: u64,
	/// Number of blocks currently verifying
	pub verifying: u64,
	/// Number of blocks in the storage
	pub stored: u64,
}

/// Blockchain from synchroniation point of view, consisting of:
/// 1) all blocks from the `storage` [oldest blocks]
/// 2) all blocks currently verifying by `verification_queue`
/// 3) all blocks currently requested from peers
/// 4) all blocks currently scheduled for requesting [newest blocks]
pub struct Chain {
	/// Genesis block hash (stored for optimizations)
	genesis_block_hash: H256,
	/// Best storage block hash (stored for optimizations)
	best_storage_block_hash: H256,
	/// Local blocks storage
	storage: Arc<db::Store>,
	/// Verification queue
	verification_queue: verification::Queue,
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
		let best_storage_block_number = storage.best_block_number()
			.expect("non-empty storage is required");
		let best_storage_block_hash = storage.block_hash(best_storage_block_number)
			.expect("checked above");

		// default verification queue
		let verifier = Box::new(verification::ChainVerifier::new(storage.clone()));
		let verification_queue = verification::Queue::new(verifier);

		Chain {
			genesis_block_hash: genesis_block_hash,
			best_storage_block_hash: best_storage_block_hash,
			storage: storage,
			verification_queue: verification_queue,
			hash_chain: HashQueueChain::with_number_of_queues(NUMBER_OF_QUEUES),
		}
	}

	#[cfg(test)]
	/// Create new `Chain` with in-memory test storage
	pub fn with_test_storage() -> Self {
		// we only work with storages with genesis block
		let storage = Arc::new(db::TestStorage::with_genesis_block());
		let genesis_block_hash = storage.block_hash(0)
			.expect("storage with genesis block is required");
		let best_storage_block_number = storage.best_block_number()
			.expect("non-empty storage is required");
		let best_storage_block_hash = storage.block_hash(best_storage_block_number)
			.expect("checked above");

		// default verification queue
		let verifier = Box::new(verification::ChainVerifier::new(storage.clone()));
		let verification_queue = verification::Queue::new(verifier);

		Chain {
			genesis_block_hash: genesis_block_hash,
			best_storage_block_hash: best_storage_block_hash,
			storage: storage,
			verification_queue: verification_queue,
			hash_chain: HashQueueChain::with_number_of_queues(NUMBER_OF_QUEUES),
		}
	}

	/// Get information on current blockchain state
	pub fn information(&self) -> Information {
		Information {
			scheduled: self.hash_chain.len_of(SCHEDULED_QUEUE) as u64,
			requested: self.hash_chain.len_of(REQUESTED_QUEUE) as u64,
			verifying: self.hash_chain.len_of(VERIFYING_QUEUE) as u64,
			stored: self.storage.best_block_number().map_or(0, |number| number + 1),
		}
	}

	/// Get total blockchain length
	pub fn length(&self) -> u64 {
		self.storage.best_block_number().expect("storage with genesis block is required") + 1
			+ self.hash_chain.len() as u64
	}

	/// Get number of blocks in given state
	pub fn length_of_state(&self, state: BlockState) -> u64 {
		self.hash_chain.len_of(state.to_queue_index()) as u64
	}

	/// Returns true if has blocks of given type
	pub fn has_blocks_of_state(&self, state: BlockState) -> bool {
		!self.hash_chain.is_empty_at(state.to_queue_index())
	}

	/// Get best block
	pub fn best_block(&self) -> BestBlock {
		let storage_best_block_number = self.storage.best_block_number().expect("storage with genesis block is required");
		match self.hash_chain.back() {
			Some(hash) => BestBlock {
				height: storage_best_block_number + self.hash_chain.len() as u64,
				hash: hash.clone(),
			},
			None => BestBlock {
				height: storage_best_block_number,
				hash: self.storage.block_hash(storage_best_block_number).expect("storage with genesis block is required"),
			}
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
		vec![self.best_block().hash]
	}

	/// Prepare block locator hashes, as described in protocol documentation:
	/// https://en.bitcoin.it/wiki/Protocol_documentation#getblocks
	pub fn block_locator_hashes(&self) -> Vec<H256> {
		let mut block_locator_hashes: Vec<H256> = Vec::new();

		// calculate for hash_queue
		let (local_index, step) = self.block_locator_hashes_for(0, 1, SCHEDULED_QUEUE, &mut block_locator_hashes);
		let (local_index, step) = self.block_locator_hashes_for(local_index, step, REQUESTED_QUEUE, &mut block_locator_hashes);
		let (local_index, step) = self.block_locator_hashes_for(local_index, step, VERIFYING_QUEUE, &mut block_locator_hashes);
		// calculate for storage
		let storage_best_block_number = self.storage.best_block_number().expect("storage with genesis block is required");
		let storage_index = if storage_best_block_number < local_index { 0 } else { storage_best_block_number - local_index };
		self.block_locator_hashes_for_storage(storage_index, step, &mut block_locator_hashes);
		block_locator_hashes
	}

	/// Schedule blocks for requesting
	pub fn schedule_blocks_hashes(&mut self, hashes: Vec<H256>) {
		self.hash_chain.push_back_n_at(SCHEDULED_QUEUE, hashes)
	}

	/// Pops block with givent state
	pub fn request_blocks_hashes(&mut self, num_blocks: u64) -> Vec<H256> {
		let scheduled = self.hash_chain.pop_front_n_at(SCHEDULED_QUEUE, num_blocks as usize);
		self.hash_chain.push_back_n_at(REQUESTED_QUEUE, scheduled.clone());
		scheduled
	}

	/// Schedule block for verification
	pub fn verify_and_insert_block(&mut self, hash: H256, block: Block) {
		// TODO: add another basic verifications here (use verification package)
		// TODO: return error if basic verification failed && reset synchronization state
		if block.block_header.previous_header_hash != self.best_storage_block_hash {
			// TODO: penalize peer
			// if let Some(peer_index) = peer_index {
			//	peers.on_wrong_block_received(peer_index);
			// }
			trace!(target: "sync", "Out-of order block dropped: {:?}", hash);
			return;
		}

		self.storage.insert_block(&block);
		self.best_storage_block_hash = hash;

		/* TODO: fails on first 500 blocks
		// TODO: async verification
		match self.verification_queue.push(block) {
			Err(err) => {
				// TODO: penalize peer
				trace!(target: "sync", "Cannot push block {:?} to verification queue: {:?}", hash, err);
				unimplemented!();
			},
			_ => (), 
		}
		self.verification_queue.process();
		match self.verification_queue.pop_valid() {
			Some((hash, block)) => {
				// TODO: check block.chain here when working on forks
				match self.storage.insert_block(&block.block) {
					Err(err) => {
						trace!(target: "sync", "Cannot insert block {:?} to database: {:?}", hash, err);
						unimplemented!();
					},
					_ => {
						self.best_storage_block_hash = hash;
					},
				}
			},
			_ => {
				trace!(target: "sync", "Error verifying block {:?}: {:?}", hash, self.verification_queue.block_status(&hash));
				unimplemented!();
			},
		}*/
	}

	/// Remove block by hash if it is currently in given state
	pub fn remove_block_with_state(&mut self, hash: &H256, state: BlockState) -> HashPosition {
		self.hash_chain.remove_at(state.to_queue_index(), hash)
	}

	/// Calculate block locator hashes for qiven hash queue
	fn block_locator_hashes_for(&self, local_index: u64, mut step: u64, queue_index: usize, hashes: &mut Vec<H256>) -> (u64, u64) {
		let queue = self.hash_chain.queue_at(queue_index);
		let queue_len = queue.len() as u64;

		// no items in queue => proceed to next storage
		if queue_len == 0 {
			return (local_index, step);
		}

		// there are less items in the queue than we need to skip => proceed to next storage
		if queue_len - 1 < local_index {
			return (local_index - queue_len - 1, step);
		}

		let mut local_index = queue_len - 1 - local_index;
		loop {
			let hash = queue[local_index as usize].clone();
			hashes.push(hash);

			if hashes.len() >= 10 {
				step <<= 1;
			}
			if local_index < step {
				return (step - local_index - 1, step);
			}
			local_index -= step;
		}
	}

	/// Calculate block locator hashes for storage
	fn block_locator_hashes_for_storage(&self, mut index: u64, mut step: u64, hashes: &mut Vec<H256>) {
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
