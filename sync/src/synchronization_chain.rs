use std::fmt;
use std::sync::Arc;
use std::collections::VecDeque;
use linked_hash_map::LinkedHashMap;
use parking_lot::RwLock;
use chain::{BlockHeader, Transaction};
use db::{self, IndexedBlock};
use best_headers_chain::{BestHeadersChain, Information as BestHeadersInformation};
use primitives::bytes::Bytes;
use primitives::hash::H256;
use hash_queue::{HashQueueChain, HashPosition};
use miner::{MemoryPool, MemoryPoolOrderingStrategy, MemoryPoolInformation};

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

/// Block insertion result
#[derive(Debug, Default, PartialEq)]
pub struct BlockInsertionResult {
	/// Hashes of blocks, which were canonized during this insertion procedure. Order matters
	pub canonized_blocks_hashes: Vec<H256>,
	/// Transaction to 'reverify'. Order matters
	pub transactions_to_reverify: Vec<(H256, Transaction)>,
}

impl BlockInsertionResult {
	#[cfg(test)]
	pub fn with_canonized_blocks(canonized_blocks_hashes: Vec<H256>) -> Self {
		BlockInsertionResult {
			canonized_blocks_hashes: canonized_blocks_hashes,
			transactions_to_reverify: Vec::new(),
		}
	}
}

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

/// Transactions synchronization state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TransactionState {
	/// Transaction is unknown
	Unknown,
	/// Currently verifying
	Verifying,
	/// In memory pool
	InMemory,
	/// In storage
	Stored,
}

/// Synchronization chain information
pub struct Information {
	/// Number of blocks hashes currently scheduled for requesting
	pub scheduled: u32,
	/// Number of blocks hashes currently requested from peers
	pub requested: u32,
	/// Number of blocks currently verifying
	pub verifying: u32,
	/// Number of blocks in the storage
	pub stored: u32,
	/// Information on memory pool
	pub transactions: MemoryPoolInformation,
	/// Information on headers chain
	pub headers: BestHeadersInformation,
}

/// Result of intersecting chain && inventory
#[derive(Debug, PartialEq)]
pub enum HeadersIntersection {
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
	storage: db::SharedStore,
	/// In-memory queue of blocks hashes
	hash_chain: HashQueueChain,
	/// In-memory queue of blocks headers
	headers_chain: BestHeadersChain,
	/// Currently verifying transactions
	verifying_transactions: LinkedHashMap<H256, Transaction>,
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
	pub fn new(storage: db::SharedStore) -> Self {
		// we only work with storages with genesis block
		let genesis_block_hash = storage.block_hash(0)
			.expect("storage with genesis block is required");
		let best_storage_block = storage.best_block()
			.expect("non-empty storage is required");
		let best_storage_block_hash = best_storage_block.hash.clone();

		Chain {
			genesis_block_hash: genesis_block_hash,
			best_storage_block: best_storage_block,
			storage: storage,
			hash_chain: HashQueueChain::with_number_of_queues(NUMBER_OF_QUEUES),
			headers_chain: BestHeadersChain::new(best_storage_block_hash),
			verifying_transactions: LinkedHashMap::new(),
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
			transactions: self.memory_pool.information(),
			headers: self.headers_chain.information(),
		}
	}

	/// Get storage
	pub fn storage(&self) -> db::SharedStore {
		self.storage.clone()
	}

	/// Get memory pool
	pub fn memory_pool(&self) -> &MemoryPool {
		&self.memory_pool
	}

	/// Get number of blocks in given state
	pub fn length_of_blocks_state(&self, state: BlockState) -> u32 {
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

	/// Get best storage block
	pub fn best_storage_block(&self) -> db::BestBlock {
		self.best_storage_block.clone()
	}

	/// Get best block header
	pub fn best_block_header(&self) -> db::BestBlock {
		let headers_chain_information = self.headers_chain.information();
		if headers_chain_information.best == 0 {
			return self.best_storage_block()
		}
		db::BestBlock {
			number: self.best_storage_block.number + headers_chain_information.best,
			hash: self.headers_chain.at(headers_chain_information.best - 1)
				.expect("got this index above; qed")
				.hash(),
		}
	}

	/// Get block header by hash
	pub fn block_hash(&self, number: u32) -> Option<H256> {
		if number <= self.best_storage_block.number {
			self.storage.block_hash(number)
		} else {
			// we try to keep these in order, but they are probably not
			self.hash_chain.at(number - self.best_storage_block.number)
		}
	}

	/// Get block number by hash
	pub fn block_number(&self, hash: &H256) -> Option<u32> {
		if let Some(number) = self.storage.block_number(hash) {
			return Some(number);
		}
		self.headers_chain.height(hash).map(|p| self.best_storage_block.number + p + 1)
	}

	/// Get block header by number
	pub fn block_header_by_number(&self, number: u32) -> Option<BlockHeader> {
		if number <= self.best_storage_block.number {
			self.storage.block_header(db::BlockRef::Number(number))
		} else {
			self.headers_chain.at(number - self.best_storage_block.number)
		}
	}

	/// Get block header by hash
	pub fn block_header_by_hash(&self, hash: &H256) -> Option<BlockHeader> {
		if let Some(block) = self.storage.block(db::BlockRef::Hash(hash.clone())) {
			return Some(block.block_header);
		}
		self.headers_chain.by_hash(hash)
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

	/// Schedule blocks hashes for requesting
	pub fn schedule_blocks_headers(&mut self, hashes: Vec<H256>, headers: Vec<BlockHeader>) {
		self.hash_chain.push_back_n_at(SCHEDULED_QUEUE, hashes);
		self.headers_chain.insert_n(headers);
	}

	/// Moves n blocks from scheduled queue to requested queue
	pub fn request_blocks_hashes(&mut self, n: u32) -> Vec<H256> {
		let scheduled = self.hash_chain.pop_front_n_at(SCHEDULED_QUEUE, n);
		self.hash_chain.push_back_n_at(REQUESTED_QUEUE, scheduled.clone());
		scheduled
	}

	/// Add block to verifying queue
	pub fn verify_block(&mut self, hash: H256, header: BlockHeader) {
		// insert header to the in-memory chain in case when it is not already there (non-headers-first sync)
		self.headers_chain.insert(header);
		self.hash_chain.push_back_at(VERIFYING_QUEUE, hash);
	}

	/// Add blocks to verifying queue
	pub fn verify_blocks(&mut self, blocks: Vec<(H256, BlockHeader)>) {
		for (hash, header) in blocks {
			self.verify_block(hash, header);
		}
	}

	/// Moves n blocks from requested queue to verifying queue
	#[cfg(test)]
	pub fn verify_blocks_hashes(&mut self, n: u32) -> Vec<H256> {
		let requested = self.hash_chain.pop_front_n_at(REQUESTED_QUEUE, n);
		self.hash_chain.push_back_n_at(VERIFYING_QUEUE, requested.clone());
		requested
	}

	/// Insert new best block to storage
	pub fn insert_best_block(&mut self, hash: H256, block: &IndexedBlock) -> Result<BlockInsertionResult, db::Error> {
		let is_appending_to_main_branch = self.best_storage_block.hash == block.header().previous_header_hash;

		// insert to storage
		let storage_insertion = try!(self.storage.insert_indexed_block(&block));

		// remember new best block hash
		self.best_storage_block = self.storage.best_block().expect("Inserted block above");

		// remove inserted block + handle possible reorganization in headers chain
		self.headers_chain.block_inserted_to_storage(&hash, &self.best_storage_block.hash);

		// case 1: block has been added to the main branch
		if is_appending_to_main_branch {
			// double check
			assert_eq!(self.best_storage_block.hash, hash);

			// all transactions from this block were accepted
			// => delete accepted transactions from verification queue and from the memory pool
			let this_block_transactions_hashes: Vec<H256> = block.transaction_hashes().iter().cloned().collect();
			for transaction_accepted in this_block_transactions_hashes {
				self.memory_pool.remove_by_hash(&transaction_accepted);
				self.verifying_transactions.remove(&transaction_accepted);
			}
			// no transactions to reverify, because we have just appended new transactions to the blockchain

			Ok(BlockInsertionResult {
				canonized_blocks_hashes: vec![hash],
				transactions_to_reverify: Vec::new(),
			})
		}
		// case 2: block has been added to the side branch with reorganization to this branch
		else if self.best_storage_block.hash == hash {
			let mut reorganization = match storage_insertion {
				db::BlockInsertedChain::Reorganized(reorganization) => reorganization,
				// we have just inserted block to side chain (!is_appending_to_main_branch)
				// && it became best block (self.best_storage_block.hash == hash)
				// => we expect db::BlockInsertedChain::Reorganized here
				_ => unreachable!(),
			};

			// all transactions from this block were accepted
			// + all transactions from previous blocks of this fork were accepted
			// => delete accepted transactions from verification queue and from the memory pool
			let this_block_transactions_hashes: Vec<H256> = block.transaction_hashes().iter().cloned().collect();
			let mut canonized_blocks_hashes: Vec<H256> = Vec::new();
			let mut new_main_blocks_transactions_hashes: Vec<H256> = Vec::new();
			while let Some(canonized_block_hash) = reorganization.pop_canonized() {
				let canonized_transactions_hashes = self.storage.block_transaction_hashes(db::BlockRef::Hash(canonized_block_hash.clone()));
				new_main_blocks_transactions_hashes.extend(canonized_transactions_hashes);
				canonized_blocks_hashes.push(canonized_block_hash);
			}
			for transaction_accepted in this_block_transactions_hashes.into_iter().chain(new_main_blocks_transactions_hashes.into_iter()) {
				self.memory_pool.remove_by_hash(&transaction_accepted);
				self.verifying_transactions.remove(&transaction_accepted);
			}
			canonized_blocks_hashes.reverse();

			// reverify all transactions from old main branch' blocks
			let mut old_main_blocks_transactions_hashes: Vec<H256> = Vec::new();
			while let Some(decanonized_block_hash) = reorganization.pop_decanonized() {
				let decanonized_transactions_hashes = self.storage.block_transaction_hashes(db::BlockRef::Hash(decanonized_block_hash));
				old_main_blocks_transactions_hashes.extend(decanonized_transactions_hashes);
			}
			let old_main_blocks_transactions: Vec<(H256, Transaction)> = old_main_blocks_transactions_hashes.into_iter()
				.map(|h| (h.clone(), self.storage.transaction(&h).expect("block in storage => block transaction in storage")))
				.collect();

			// reverify memory pool transactions
			// TODO: maybe reverify only transactions, which depends on other reverifying transactions + transactions from new main branch?
			let memory_pool_transactions_count = self.memory_pool.information().transactions_count;
			let memory_pool_transactions: Vec<_> = self.memory_pool
				.remove_n_with_strategy(memory_pool_transactions_count, MemoryPoolOrderingStrategy::ByTimestamp)
				.into_iter()
				.map(|t| (t.hash(), t))
				.collect();

			// reverify verifying transactions
			let verifying_transactions: Vec<_> = self.verifying_transactions
				.iter()
				.map(|(h, t)| (h.clone(), t.clone()))
				.collect();
			// there's no guarantee (in docs) that LinkedHashMap::into_iter() will return values ordered by insertion time
			self.verifying_transactions.clear();

			Ok(BlockInsertionResult {
				canonized_blocks_hashes: canonized_blocks_hashes,
				// order matters: db transactions, then ordered mempool transactions, then ordered verifying transactions
				transactions_to_reverify: old_main_blocks_transactions.into_iter()
					.chain(memory_pool_transactions.into_iter())
					.chain(verifying_transactions.into_iter())
					.collect(),
			})
		}
		// case 3: block has been added to the side branch without reorganization to this branch
		else {
			// no transactions were accepted
			// no transactions to reverify
			Ok(BlockInsertionResult::default())
		}
	}

	/// Forget in-memory block
	pub fn forget_block(&mut self, hash: &H256) -> HashPosition {
		self.headers_chain.remove(hash);
		self.forget_block_leave_header(hash)
	}

	/// Forget in-memory blocks
	pub fn forget_blocks(&mut self, hashes: &[H256]) {
		for hash in hashes {
			self.forget_block(hash);
		}
	}

	/// Forget in-memory block, but leave its header in the headers_chain (orphan queue)
	pub fn forget_block_leave_header(&mut self, hash: &H256) -> HashPosition {
		match self.hash_chain.remove_at(VERIFYING_QUEUE, hash) {
			HashPosition::Missing => match self.hash_chain.remove_at(REQUESTED_QUEUE, hash) {
				HashPosition::Missing => self.hash_chain.remove_at(SCHEDULED_QUEUE, hash),
				position => position,
			},
			position => position,
		}
	}

	/// Forget in-memory blocks, but leave their headers in the headers_chain (orphan queue)
	pub fn forget_blocks_leave_header(&mut self, hashes: &[H256]) {
		for hash in hashes {
			self.forget_block_leave_header(hash);
		}
	}

	/// Forget in-memory block by hash if it is currently in given state
	#[cfg(test)]
	pub fn forget_block_with_state(&mut self, hash: &H256, state: BlockState) -> HashPosition {
		self.headers_chain.remove(hash);
		self.forget_block_with_state_leave_header(hash, state)
	}

	/// Forget in-memory block by hash if it is currently in given state
	pub fn forget_block_with_state_leave_header(&mut self, hash: &H256, state: BlockState) -> HashPosition {
		self.hash_chain.remove_at(state.to_queue_index(), hash)
	}

	/// Forget in-memory block by hash.
	/// Also forget all its known children.
	pub fn forget_block_with_children(&mut self, hash: &H256) {
		let mut removal_stack: VecDeque<H256> = VecDeque::new();
		let mut removal_queue: VecDeque<H256> = VecDeque::new();
		removal_queue.push_back(hash.clone());

		// remove in reverse order to minimize headers operations
		while let Some(hash) = removal_queue.pop_front() {
			removal_queue.extend(self.headers_chain.children(&hash));
			removal_stack.push_back(hash);
		}
		while let Some(hash) = removal_stack.pop_back() {
			self.forget_block(&hash);
		}
	}

	/// Forget all blocks with given state
	pub fn forget_all_blocks_with_state(&mut self, state: BlockState) {
		let hashes = self.hash_chain.remove_all_at(state.to_queue_index());
		self.headers_chain.remove_n(hashes);
	}

	/// Intersect chain with inventory
	pub fn intersect_with_blocks_headers(&self, hashes: &[H256], headers: &[BlockHeader]) -> HeadersIntersection {
		let hashes_len = hashes.len();
		assert!(hashes_len != 0 && hashes.len() == headers.len());

		// giving that headers are ordered
		let (is_first_known, first_state) = match self.block_state(&hashes[0]) {
			BlockState::Unknown => (false, self.block_state(&headers[0].previous_header_hash)),
			state => (true, state),
		};
		match first_state {
			// if first block of inventory is unknown && its parent is unknonw => all other blocks are also unknown
			BlockState::Unknown => {
				HeadersIntersection::NoKnownBlocks(0)
			},
			// else if first block is known
			first_block_state => match self.block_state(&hashes[hashes_len - 1]) {
				// if last block is known to be in db => all inventory blocks are also in db
				BlockState::Stored => {
					HeadersIntersection::DbAllBlocksKnown
				},
				// if first block is known && last block is unknown but we know block before first one => intersection with queue or with db
				BlockState::Unknown if !is_first_known => {
					// previous block is stored => fork from stored block
					if first_state == BlockState::Stored {
						HeadersIntersection::DbForkNewBlocks(0)
					}
					// previous block is best block => no fork
					else if &self.best_block().hash == &headers[0].previous_header_hash {
						HeadersIntersection::InMemoryMainNewBlocks(0)
					}
					// previous block is not a best block => fork
					else {
						HeadersIntersection::InMemoryForkNewBlocks(0)
					}
				},
				// if first block is known && last block is unknown => intersection with queue or with db
				BlockState::Unknown if is_first_known => {
					// find last known block
					let mut previous_state = first_block_state;
					for (index, hash) in hashes.iter().enumerate().take(hashes_len).skip(1) {
						let state = self.block_state(hash);
						if state == BlockState::Unknown {
							// previous block is stored => fork from stored block
							if previous_state == BlockState::Stored {
								return HeadersIntersection::DbForkNewBlocks(index);
							}
							// previous block is best block => no fork
							else if &self.best_block().hash == &hashes[index - 1] {
								return HeadersIntersection::InMemoryMainNewBlocks(index);
							}
							// previous block is not a best block => fork
							else {
								return HeadersIntersection::InMemoryForkNewBlocks(index);
							}
						}
						previous_state = state;
					}

					// unreachable because last block is unknown && in above loop we search for unknown blocks
					unreachable!();
				},
				// if first block is known && last block is also known && is in queue => queue intersection with no new block
				_ => {
					HeadersIntersection::InMemoryNoNewBlocks
				}
			}
		}
	}

	/// Get transaction state
	pub fn transaction_state(&self, hash: &H256) -> TransactionState {
		if self.verifying_transactions.contains_key(hash) {
			return TransactionState::Verifying;
		}
		if self.memory_pool.contains(hash) {
			return TransactionState::InMemory;
		}
		if self.storage.contains_transaction(hash) {
			return TransactionState::Stored;
		}
		TransactionState::Unknown
	}

	/// Get transactions hashes with given state
	pub fn transactions_hashes_with_state(&self, state: TransactionState) -> Vec<H256> {
		match state {
			TransactionState::InMemory => self.memory_pool.get_transactions_ids(),
			TransactionState::Verifying => self.verifying_transactions.keys().cloned().collect(),
			_ => panic!("wrong argument"),
		}
	}

	/// Add transaction to verifying queue
	pub fn verify_transaction(&mut self, hash: H256, tx: Transaction) {
		self.verifying_transactions.insert(hash, tx);
	}

	/// Remove verifying trasaction
	pub fn forget_verifying_transaction(&mut self, hash: &H256) -> bool {
		self.verifying_transactions.remove(hash).is_some()
	}

	/// Remove verifying trasaction + all dependent transactions currently verifying
	pub fn forget_verifying_transaction_with_children(&mut self, hash: &H256) {
		self.forget_verifying_transaction(hash);

		// TODO: suboptimal
		let mut queue: VecDeque<H256> = VecDeque::new();
		queue.push_back(hash.clone());
		while let Some(hash) = queue.pop_front() {
			let all_keys: Vec<_> = self.verifying_transactions.keys().cloned().collect();
			for h in all_keys {
				let remove_verifying_transaction = {
					if let Some(entry) = self.verifying_transactions.get(&h) {
						if entry.inputs.iter().any(|i| i.previous_output.hash == hash) {
							queue.push_back(h.clone());
							true
						} else {
							false
						}
					} else {
						// iterating by previously read keys
						unreachable!()
					}
				};

				if remove_verifying_transaction {
					self.verifying_transactions.remove(&h);
				}
			}
		}
	}

	/// Get transaction by hash (if it's in memory pool or verifying)
	pub fn transaction_by_hash(&self, hash: &H256) -> Option<Transaction> {
		self.verifying_transactions.get(hash).cloned()
			.or_else(|| self.memory_pool.read_by_hash(hash).cloned())
	}

	/// Insert transaction to memory pool
	pub fn insert_verified_transaction(&mut self, transaction: Transaction) {
		self.memory_pool.insert_verified(transaction);
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

impl db::TransactionProvider for Chain {
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.memory_pool.transaction_bytes(hash)
			.or_else(|| self.storage.transaction_bytes(hash))
	}

	fn transaction(&self, hash: &H256) -> Option<Transaction> {
		self.memory_pool.transaction(hash)
			.or_else(|| self.storage.transaction(hash))
	}
}

impl fmt::Debug for Information {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "[sch:{} / bh:{} -> req:{} -> vfy:{} -> stored: {}]", self.scheduled, self.headers.best, self.requested, self.verifying, self.stored)
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
	use chain::Transaction;
	use hash_queue::HashPosition;
	use super::{Chain, BlockState, TransactionState, HeadersIntersection, BlockInsertionResult};
	use db::{self, Store, BestBlock};
	use primitives::hash::H256;
	use devtools::RandomTempPath;
	use test_data;
	use db::BlockStapler;

	#[test]
	fn chain_empty() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let db_best_block = BestBlock { number: 0, hash: db.best_block().expect("storage with genesis block is required").hash };
		let chain = Chain::new(db.clone());
		assert_eq!(chain.information().scheduled, 0);
		assert_eq!(chain.information().requested, 0);
		assert_eq!(chain.information().verifying, 0);
		assert_eq!(chain.information().stored, 1);
		assert_eq!(chain.length_of_blocks_state(BlockState::Scheduled), 0);
		assert_eq!(chain.length_of_blocks_state(BlockState::Requested), 0);
		assert_eq!(chain.length_of_blocks_state(BlockState::Verifying), 0);
		assert_eq!(chain.length_of_blocks_state(BlockState::Stored), 1);
		assert_eq!(&chain.best_block(), &db_best_block);
		assert_eq!(chain.block_state(&db_best_block.hash), BlockState::Stored);
		assert_eq!(chain.block_state(&H256::from(0)), BlockState::Unknown);
	}

	#[test]
	fn chain_block_path() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut chain = Chain::new(db.clone());

		// add 6 blocks to scheduled queue
		let blocks = test_data::build_n_empty_blocks_from_genesis(6, 0);
		let headers: Vec<_> = blocks.into_iter().map(|b| b.block_header).collect();
		let hashes: Vec<_> = headers.iter().map(|h| h.hash()).collect();
		chain.schedule_blocks_headers(hashes.clone(), headers);
		assert!(chain.information().scheduled == 6 && chain.information().requested == 0
			&& chain.information().verifying == 0 && chain.information().stored == 1);

		// move 2 best blocks to requested queue
		chain.request_blocks_hashes(2);
		assert!(chain.information().scheduled == 4 && chain.information().requested == 2
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// move 0 best blocks to requested queue
		chain.request_blocks_hashes(0);
		assert!(chain.information().scheduled == 4 && chain.information().requested == 2
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// move 1 best blocks to requested queue
		chain.request_blocks_hashes(1);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 3
			&& chain.information().verifying == 0 && chain.information().stored == 1);

		// try to remove block 0 from scheduled queue => missing
		assert_eq!(chain.forget_block_with_state(&hashes[0], BlockState::Scheduled), HashPosition::Missing);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 3
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// remove blocks 0 & 1 from requested queue
		assert_eq!(chain.forget_block_with_state(&hashes[1], BlockState::Requested), HashPosition::Inside(1));
		assert_eq!(chain.forget_block_with_state(&hashes[0], BlockState::Requested), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 0 && chain.information().stored == 1);
		// mark 0 & 1 as verifying
		chain.verify_block(hashes[1].clone(), test_data::genesis().block_header);
		chain.verify_block(hashes[2].clone(), test_data::genesis().block_header);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 2 && chain.information().stored == 1);

		// mark block 0 as verified
		assert_eq!(chain.forget_block_with_state(&hashes[1], BlockState::Verifying), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 1);
		// insert new best block to the chain
		chain.insert_best_block(test_data::block_h1().hash(), &test_data::block_h1().into()).expect("Db error");
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

		chain.insert_best_block(block1_hash.clone(), &block1.into()).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block1_hash.clone(), genesis_hash.clone()]);

		let block2 = test_data::block_h2();
		let block2_hash = block2.hash();

		chain.insert_best_block(block2_hash.clone(), &block2.into()).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block2_hash.clone(), block1_hash.clone(), genesis_hash.clone()]);

		let blocks0 = test_data::build_n_empty_blocks_from_genesis(11, 0);
		let headers0: Vec<_> = blocks0.into_iter().map(|b| b.block_header).collect();
		let hashes0: Vec<_> = headers0.iter().map(|h| h.hash()).collect();
		chain.schedule_blocks_headers(hashes0.clone(), headers0.clone());
		chain.request_blocks_hashes(10);
		chain.verify_blocks_hashes(10);

		assert_eq!(chain.block_locator_hashes(), vec![
			hashes0[10].clone(),
			hashes0[9].clone(),
			hashes0[8].clone(),
			hashes0[7].clone(),
			hashes0[6].clone(),
			hashes0[5].clone(),
			hashes0[4].clone(),
			hashes0[3].clone(),
			hashes0[2].clone(),
			hashes0[1].clone(),
			block2_hash.clone(),
			genesis_hash.clone(),
		]);

		let blocks1 = test_data::build_n_empty_blocks_from(6, 0, &headers0[10]);
		let headers1: Vec<_> = blocks1.into_iter().map(|b| b.block_header).collect();
		let hashes1: Vec<_> = headers1.iter().map(|h| h.hash()).collect();
		chain.schedule_blocks_headers(hashes1.clone(), headers1.clone());
		chain.request_blocks_hashes(10);

		assert_eq!(chain.block_locator_hashes(), vec![
			hashes1[5].clone(),
			hashes1[4].clone(),
			hashes1[3].clone(),
			hashes1[2].clone(),
			hashes1[1].clone(),
			hashes1[0].clone(),
			hashes0[10].clone(),
			hashes0[9].clone(),
			hashes0[8].clone(),
			hashes0[7].clone(),
			hashes0[5].clone(),
			hashes0[1].clone(),
			genesis_hash.clone(),
		]);

		let blocks2 = test_data::build_n_empty_blocks_from(3, 0, &headers1[5]);
		let headers2: Vec<_> = blocks2.into_iter().map(|b| b.block_header).collect();
		let hashes2: Vec<_> = headers2.iter().map(|h| h.hash()).collect();
		chain.schedule_blocks_headers(hashes2.clone(), headers2);

		assert_eq!(chain.block_locator_hashes(), vec![
			hashes2[2].clone(),
			hashes2[1].clone(),
			hashes2[0].clone(),
			hashes1[5].clone(),
			hashes1[4].clone(),
			hashes1[3].clone(),
			hashes1[2].clone(),
			hashes1[1].clone(),
			hashes1[0].clone(),
			hashes0[10].clone(),
			hashes0[8].clone(),
			hashes0[4].clone(),
			genesis_hash.clone(),
		]);
	}

	#[test]
	fn chain_intersect_with_inventory() {
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		// append 2 db blocks
		chain.insert_best_block(test_data::block_h1().hash(), &test_data::block_h1().into()).expect("Error inserting new block");
		chain.insert_best_block(test_data::block_h2().hash(), &test_data::block_h2().into()).expect("Error inserting new block");

		// prepare blocks
		let blocks0 = test_data::build_n_empty_blocks_from(9, 0, &test_data::block_h2().block_header);
		let headers0: Vec<_> = blocks0.into_iter().map(|b| b.block_header).collect();
		let hashes0: Vec<_> = headers0.iter().map(|h| h.hash()).collect();
		// append 3 verifying blocks, 3 requested blocks && 3 scheduled blocks
		chain.schedule_blocks_headers(hashes0.clone(), headers0.clone());
		chain.request_blocks_hashes(6);
		chain.verify_blocks_hashes(3);

		let blocks1 = test_data::build_n_empty_blocks(2, 0);
		let headers1: Vec<_> = blocks1.into_iter().map(|b| b.block_header).collect();
		let hashes1: Vec<_> = headers1.iter().map(|h| h.hash()).collect();
		assert_eq!(chain.intersect_with_blocks_headers(&hashes1, &headers1), HeadersIntersection::NoKnownBlocks(0));

		assert_eq!(chain.intersect_with_blocks_headers(&vec![
			hashes0[2].clone(),
			hashes0[3].clone(),
			hashes0[4].clone(),
			hashes0[5].clone(),
			hashes0[6].clone(),
		], &vec![
			headers0[2].clone(),
			headers0[3].clone(),
			headers0[4].clone(),
			headers0[5].clone(),
			headers0[6].clone(),
		]), HeadersIntersection::InMemoryNoNewBlocks);

		assert_eq!(chain.intersect_with_blocks_headers(&vec![
			hashes0[7].clone(),
			hashes0[8].clone(),
			hashes1[0].clone(),
			hashes1[1].clone(),
		], &vec![
			headers0[7].clone(),
			headers0[8].clone(),
			headers1[0].clone(),
			headers1[1].clone(),
		]), HeadersIntersection::InMemoryMainNewBlocks(2));

		assert_eq!(chain.intersect_with_blocks_headers(&vec![
			hashes0[6].clone(),
			hashes0[7].clone(),
			hashes1[0].clone(),
			hashes1[1].clone(),
		], &vec![
			headers0[6].clone(),
			headers0[7].clone(),
			headers1[0].clone(),
			headers1[1].clone(),
		]), HeadersIntersection::InMemoryForkNewBlocks(2));

		assert_eq!(chain.intersect_with_blocks_headers(&vec![
			test_data::block_h1().hash(),
			test_data::block_h2().hash(),
		], &vec![
			test_data::block_h1().block_header,
			test_data::block_h2().block_header,
		]), HeadersIntersection::DbAllBlocksKnown);

		assert_eq!(chain.intersect_with_blocks_headers(&vec![
			test_data::block_h2().hash(),
			hashes1[0].clone(),
		], &vec![
			test_data::block_h2().block_header,
			headers1[0].clone(),
		]), HeadersIntersection::DbForkNewBlocks(1));
	}

	#[test]
	fn chain_transaction_state() {
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		let genesis_block = test_data::genesis();
		let block1 = test_data::block_h1();
		let tx1: Transaction = test_data::TransactionBuilder::with_version(1).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_version(2).into();
		let tx1_hash = tx1.hash();
		let tx2_hash = tx2.hash();
		chain.verify_transaction(tx1_hash.clone(), tx1);
		chain.insert_verified_transaction(tx2);

		assert_eq!(chain.transaction_state(&genesis_block.transactions[0].hash()), TransactionState::Stored);
		assert_eq!(chain.transaction_state(&block1.transactions[0].hash()), TransactionState::Unknown);
		assert_eq!(chain.transaction_state(&tx1_hash), TransactionState::Verifying);
		assert_eq!(chain.transaction_state(&tx2_hash), TransactionState::InMemory);
	}

	#[test]
	fn chain_block_transaction_is_removed_from_on_block_insert() {
		let b0 = test_data::block_builder().header().build().build();
		let b1 = test_data::block_builder().header().parent(b0.hash()).build()
			.transaction().coinbase()
				.output().value(10).build()
				.build()
			.transaction()
				.input().hash(H256::from(1)).index(1).build()
				.build()
			.build();
		let tx1 = b1.transactions[0].clone();
		let tx1_hash = tx1.hash();
		let tx2 = b1.transactions[1].clone();
		let tx2_hash = tx2.hash();

		let mut chain = Chain::new(Arc::new(db::TestStorage::with_blocks(&vec![b0])));
		chain.verify_transaction(tx1_hash.clone(), tx1);
		chain.insert_verified_transaction(tx2);

		// only one transaction is in the memory pool
		assert_eq!(chain.information().transactions.transactions_count, 1);

		// when block is inserted to the database => all accepted transactions are removed from mempool && verifying queue
		chain.insert_best_block(b1.hash(), &b1.into()).expect("block accepted");

		assert_eq!(chain.information().transactions.transactions_count, 0);
		assert!(!chain.forget_verifying_transaction(&tx1_hash));
		assert!(!chain.forget_verifying_transaction(&tx2_hash));
	}

	#[test]
	fn chain_forget_verifying_transaction_with_children() {
		let test_chain = &mut test_data::ChainBuilder::new();
		test_data::TransactionBuilder::with_output(100).store(test_chain)	// t1
			.into_input(0).add_output(200).store(test_chain)				// t1 -> t2
			.into_input(0).add_output(300).store(test_chain)				// t1 -> t2 -> t3
			.set_default_input(0).set_output(400).store(test_chain);		// t4

		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		chain.verify_transaction(test_chain.at(0).hash(), test_chain.at(0));
		chain.verify_transaction(test_chain.at(1).hash(), test_chain.at(1));
		chain.verify_transaction(test_chain.at(2).hash(), test_chain.at(2));
		chain.verify_transaction(test_chain.at(3).hash(), test_chain.at(3));

		chain.forget_verifying_transaction_with_children(&test_chain.at(0).hash());
		assert!(!chain.forget_verifying_transaction(&test_chain.at(0).hash()));
		assert!(!chain.forget_verifying_transaction(&test_chain.at(1).hash()));
		assert!(!chain.forget_verifying_transaction(&test_chain.at(2).hash()));
		assert!(chain.forget_verifying_transaction(&test_chain.at(3).hash()));
	}

	#[test]
	fn chain_transactions_hashes_with_state() {
		let test_chain = &mut test_data::ChainBuilder::new();
		test_data::TransactionBuilder::with_output(100).store(test_chain)	// t1
			.into_input(0).add_output(200).store(test_chain)				// t1 -> t2
			.into_input(0).add_output(300).store(test_chain)				// t1 -> t2 -> t3
			.set_default_input(0).set_output(400).store(test_chain);		// t4

		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		chain.insert_verified_transaction(test_chain.at(0));
		chain.insert_verified_transaction(test_chain.at(1));
		chain.insert_verified_transaction(test_chain.at(2));
		chain.insert_verified_transaction(test_chain.at(3));

		let chain_transactions = chain.transactions_hashes_with_state(TransactionState::InMemory);
		assert!(chain_transactions.contains(&test_chain.at(0).hash()));
		assert!(chain_transactions.contains(&test_chain.at(1).hash()));
		assert!(chain_transactions.contains(&test_chain.at(2).hash()));
		assert!(chain_transactions.contains(&test_chain.at(3).hash()));
	}

	#[test]
	fn memory_pool_transactions_are_reverified_after_reorganization() {
		let b0 = test_data::block_builder().header().build().build();
		let b1 = test_data::block_builder().header().nonce(1).parent(b0.hash()).build().build();
		let b2 = test_data::block_builder().header().nonce(2).parent(b0.hash()).build().build();
		let b3 = test_data::block_builder().header().parent(b2.hash()).build().build();

		let tx1: Transaction = test_data::TransactionBuilder::with_version(1).into();
		let tx1_hash = tx1.hash();
		let tx2: Transaction = test_data::TransactionBuilder::with_version(2).into();
		let tx2_hash = tx2.hash();

		let path = RandomTempPath::create_dir();
		let storage = Arc::new(db::Storage::new(path.as_path()).unwrap());
		storage.insert_block(&b0).expect("no db error");

		let mut chain = Chain::new(storage);
		chain.verify_transaction(tx1_hash.clone(), tx1);
		chain.insert_verified_transaction(tx2);

		// no reorg
		let result = chain.insert_best_block(b1.hash(), &b1.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 0);

		// no reorg
		let result = chain.insert_best_block(b2.hash(), &b2.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 0);

		// reorg
		let result = chain.insert_best_block(b3.hash(), &b3.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 2);
		assert!(result.transactions_to_reverify.iter().any(|&(ref h, _)| h == &tx1_hash));
		assert!(result.transactions_to_reverify.iter().any(|&(ref h, _)| h == &tx2_hash));
	}

	#[test]
	fn fork_chain_block_transaction_is_removed_from_on_block_insert() {
		let genesis = test_data::genesis();
		let b0 = test_data::block_builder().header().parent(genesis.hash()).build().build(); // genesis -> b0
		let b1 = test_data::block_builder().header().nonce(1).parent(b0.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b0 -> b1[tx1]
		let b2 = test_data::block_builder().header().parent(b1.hash()).build()
			.transaction().output().value(20).build().build()
			.build(); // genesis -> b0 -> b1[tx1] -> b2[tx2]
		let b3 = test_data::block_builder().header().nonce(2).parent(b0.hash()).build()
			.transaction().output().value(30).build().build()
			.build(); // genesis -> b0 -> b3[tx3]
		let b4 = test_data::block_builder().header().parent(b3.hash()).build()
			.transaction().output().value(40).build().build()
			.build(); // genesis -> b0 -> b3[tx3] -> b4[tx4]
		let b5 = test_data::block_builder().header().parent(b4.hash()).build()
			.transaction().output().value(50).build().build()
			.build(); // genesis -> b0 -> b3[tx3] -> b4[tx4] -> b5[tx5]

		let tx1 = b1.transactions[0].clone();
		let tx1_hash = tx1.hash();
		let tx2 = b2.transactions[0].clone();
		let tx2_hash = tx2.hash();
		let tx3 = b3.transactions[0].clone();
		let tx4 = b4.transactions[0].clone();
		let tx5 = b5.transactions[0].clone();

		let path = RandomTempPath::create_dir();
		let storage = Arc::new(db::Storage::new(path.as_path()).unwrap());
		storage.insert_block(&genesis).expect("no db error");

		let mut chain = Chain::new(storage);

		chain.insert_verified_transaction(tx3);
		chain.insert_verified_transaction(tx4);
		chain.insert_verified_transaction(tx5);

		assert_eq!(chain.insert_best_block(b0.hash(), &b0.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b0.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b1.hash(), &b1.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b1.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b2.hash(), &b2.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b2.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b3.hash(), &b3.clone().into()).expect("block accepted"), BlockInsertionResult::default());
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b4.hash(), &b4.clone().into()).expect("block accepted"), BlockInsertionResult::default());
		assert_eq!(chain.information().transactions.transactions_count, 3);
		// order matters
		let insert_result = chain.insert_best_block(b5.hash(), &b5.clone().into()).expect("block accepted");
		let transactions_to_reverify_hashes: Vec<_> = insert_result
			.transactions_to_reverify
			.into_iter()
			.map(|(h, _)| h)
			.collect();
		assert_eq!(transactions_to_reverify_hashes, vec![tx1_hash, tx2_hash]);
		assert_eq!(insert_result.canonized_blocks_hashes, vec![b3.hash(), b4.hash(), b5.hash()]);
		assert_eq!(chain.information().transactions.transactions_count, 0); // tx3, tx4, tx5 are added to the database
	}
}
