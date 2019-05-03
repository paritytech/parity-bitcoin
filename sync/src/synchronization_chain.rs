use std::collections::{VecDeque, HashSet};
use std::fmt;
use linked_hash_map::LinkedHashMap;
use chain::{IndexedBlockHeader, IndexedBlock, IndexedTransaction, OutPoint, TransactionOutput};
use storage;
use miner::{MemoryPoolOrderingStrategy, MemoryPoolInformation, FeeCalculator};
use network::ConsensusParams;
use primitives::bytes::Bytes;
use primitives::hash::H256;
use utils::{BestHeadersChain, BestHeadersChainInformation, HashQueueChain, HashPosition};
use types::{BlockHeight, StorageRef, MemoryPoolRef};

/// Index of 'verifying' queue
const VERIFYING_QUEUE: usize = 0;
/// Index of 'requested' queue
const REQUESTED_QUEUE: usize = 1;
/// Index of 'scheduled' queue
const SCHEDULED_QUEUE: usize = 2;
/// Number of hash queues
const NUMBER_OF_QUEUES: usize = 3;

/// Block insertion result
#[derive(Default, PartialEq)]
pub struct BlockInsertionResult {
	/// Hashes of blocks, which were canonized during this insertion procedure. Order matters
	pub canonized_blocks_hashes: Vec<H256>,
	/// Transaction to 'reverify'. Order matters
	pub transactions_to_reverify: Vec<IndexedTransaction>,
}

impl fmt::Debug for BlockInsertionResult {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("BlockInsertionResult")
			.field("canonized_blocks_hashes", &self.canonized_blocks_hashes.iter().map(H256::reversed).collect::<Vec<_>>())
			.field("transactions_to_reverify", &self.transactions_to_reverify)
			.finish()
	}
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
	/// This block has been marked as dead-end block
	DeadEnd,
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
	pub scheduled: BlockHeight,
	/// Number of blocks hashes currently requested from peers
	pub requested: BlockHeight,
	/// Number of blocks currently verifying
	pub verifying: BlockHeight,
	/// Number of blocks in the storage
	pub stored: BlockHeight,
	/// Information on memory pool
	pub transactions: MemoryPoolInformation,
	/// Information on headers chain
	pub headers: BestHeadersChainInformation,
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
	best_storage_block: storage::BestBlock,
	/// Local blocks storage
	storage: StorageRef,
	/// In-memory queue of blocks hashes
	hash_chain: HashQueueChain,
	/// In-memory queue of blocks headers
	headers_chain: BestHeadersChain,
	/// Currently verifying transactions
	verifying_transactions: LinkedHashMap<H256, IndexedTransaction>,
	/// Transactions memory pool
	memory_pool: MemoryPoolRef,
	/// Blocks that have been marked as dead-ends
	dead_end_blocks: HashSet<H256>,
	/// Is SegWit is possible on this chain? SegWit inventory types are used when block/tx-es are
	/// requested and this flag is true.
	is_segwit_possible: bool,
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
	pub fn new(storage: StorageRef, consensus: ConsensusParams, memory_pool: MemoryPoolRef) -> Self {
		// we only work with storages with genesis block
		let genesis_block_hash = storage.block_hash(0)
			.expect("storage with genesis block is required");
		let best_storage_block = storage.best_block();
		let best_storage_block_hash = best_storage_block.hash.clone();
		let is_segwit_possible = consensus.is_segwit_possible();

		Chain {
			genesis_block_hash: genesis_block_hash,
			best_storage_block: best_storage_block,
			storage: storage,
			hash_chain: HashQueueChain::with_number_of_queues(NUMBER_OF_QUEUES),
			headers_chain: BestHeadersChain::new(best_storage_block_hash),
			verifying_transactions: LinkedHashMap::new(),
			memory_pool: memory_pool,
			dead_end_blocks: HashSet::new(),
			is_segwit_possible,
		}
	}

	/// Get information on current blockchain state
	pub fn information(&self) -> Information {
		Information {
			scheduled: self.hash_chain.len_of(SCHEDULED_QUEUE),
			requested: self.hash_chain.len_of(REQUESTED_QUEUE),
			verifying: self.hash_chain.len_of(VERIFYING_QUEUE),
			stored: self.best_storage_block.number + 1,
			transactions: self.memory_pool.read().information(),
			headers: self.headers_chain.information(),
		}
	}

	/// Get storage
	pub fn storage(&self) -> StorageRef {
		self.storage.clone()
	}

	/// Get memory pool
	pub fn memory_pool(&self) -> MemoryPoolRef {
		self.memory_pool.clone()
	}

	/// Is segwit active
	pub fn is_segwit_possible(&self) -> bool {
		self.is_segwit_possible
	}

	/// Get number of blocks in given state
	pub fn length_of_blocks_state(&self, state: BlockState) -> BlockHeight {
		match state {
			BlockState::Stored => self.best_storage_block.number + 1,
			_ => self.hash_chain.len_of(state.to_queue_index()),
		}
	}

	/// Get n best blocks of given state
	pub fn best_n_of_blocks_state(&self, state: BlockState, n: BlockHeight) -> Vec<H256> {
		match state {
			BlockState::Scheduled | BlockState::Requested | BlockState::Verifying => self.hash_chain.front_n_at(state.to_queue_index(), n),
			_ => unreachable!("must be checked by caller"),
		}
	}

	/// Get best block
	pub fn best_block(&self) -> storage::BestBlock {
		match self.hash_chain.back() {
			Some(hash) => storage::BestBlock {
				number: self.best_storage_block.number + self.hash_chain.len(),
				hash: hash.clone(),
			},
			None => self.best_storage_block.clone(),
		}
	}

	/// Get best storage block
	pub fn best_storage_block(&self) -> storage::BestBlock {
		self.best_storage_block.clone()
	}

	/// Get best block header
	pub fn best_block_header(&self) -> storage::BestBlock {
		let headers_chain_information = self.headers_chain.information();
		if headers_chain_information.best == 0 {
			return self.best_storage_block()
		}
		storage::BestBlock {
			number: self.best_storage_block.number + headers_chain_information.best,
			hash: self.headers_chain.at(headers_chain_information.best - 1)
				.expect("got this index above; qed")
				.hash,
		}
	}

	/// Get block header by hash
	pub fn block_hash(&self, number: BlockHeight) -> Option<H256> {
		if number <= self.best_storage_block.number {
			self.storage.block_hash(number)
		} else {
			// we try to keep these in order, but they are probably not
			self.hash_chain.at(number - self.best_storage_block.number)
		}
	}

	/// Get block number by hash
	pub fn block_number(&self, hash: &H256) -> Option<BlockHeight> {
		if let Some(number) = self.storage.block_number(hash) {
			return Some(number);
		}
		self.headers_chain.height(hash).map(|p| self.best_storage_block.number + p + 1)
	}

	/// Get block header by number
	pub fn block_header_by_number(&self, number: BlockHeight) -> Option<IndexedBlockHeader> {
		if number <= self.best_storage_block.number {
			self.storage.block_header(storage::BlockRef::Number(number))
		} else {
			self.headers_chain.at(number - self.best_storage_block.number)
		}
	}

	/// Get block header by hash
	pub fn block_header_by_hash(&self, hash: &H256) -> Option<IndexedBlockHeader> {
		if let Some(header) = self.storage.block_header(storage::BlockRef::Hash(hash.clone())) {
			return Some(header);
		}
		self.headers_chain.by_hash(hash)
	}

	/// Get block state
	pub fn block_state(&self, hash: &H256) -> BlockState {
		match self.hash_chain.contains_in(hash) {
			Some(queue_index) => BlockState::from_queue_index(queue_index),
			None => if self.storage.contains_block(storage::BlockRef::Hash(hash.clone())) {
				BlockState::Stored
			} else if self.dead_end_blocks.contains(hash) {
				BlockState::DeadEnd
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
	pub fn schedule_blocks_headers(&mut self, headers: Vec<IndexedBlockHeader>) {
		self.hash_chain.push_back_n_at(SCHEDULED_QUEUE, headers.iter().map(|h| h.hash.clone()).collect());
		self.headers_chain.insert_n(headers);
	}

	/// Moves n blocks from scheduled queue to requested queue
	pub fn request_blocks_hashes(&mut self, n: BlockHeight) -> Vec<H256> {
		let scheduled = self.hash_chain.pop_front_n_at(SCHEDULED_QUEUE, n);
		self.hash_chain.push_back_n_at(REQUESTED_QUEUE, scheduled.clone());
		scheduled
	}

	/// Add block to verifying queue
	pub fn verify_block(&mut self, header: IndexedBlockHeader) {
		// insert header to the in-memory chain in case when it is not already there (non-headers-first sync)
		self.hash_chain.push_back_at(VERIFYING_QUEUE, header.hash.clone());
		self.headers_chain.insert(header);
	}

	/// Add blocks to verifying queue
	pub fn verify_blocks(&mut self, blocks: Vec<IndexedBlockHeader>) {
		for block in blocks {
			self.verify_block(block);
		}
	}

	/// Moves n blocks from requested queue to verifying queue
	#[cfg(test)]
	pub fn verify_blocks_hashes(&mut self, n: BlockHeight) -> Vec<H256> {
		let requested = self.hash_chain.pop_front_n_at(REQUESTED_QUEUE, n);
		self.hash_chain.push_back_n_at(VERIFYING_QUEUE, requested.clone());
		requested
	}

	/// Mark this block as dead end, so these tasks won't be synchronized
	pub fn mark_dead_end_block(&mut self, hash: &H256) {
		self.dead_end_blocks.insert(hash.clone());
	}

	/// Insert new best block to storage
	pub fn insert_best_block(&mut self, block: IndexedBlock) -> Result<BlockInsertionResult, storage::Error> {
		assert_eq!(Some(self.storage.best_block().hash), self.storage.block_hash(self.storage.best_block().number));
		let block_origin = self.storage.block_origin(&block.header)?;
		trace!(target: "sync", "insert_best_block {:?} origin: {:?}", block.hash().reversed(), block_origin);
		match block_origin {
			storage::BlockOrigin::KnownBlock => {
				// there should be no known blocks at this point
				unreachable!();
			},
			// case 1: block has been added to the main branch
			storage::BlockOrigin::CanonChain { .. } => {
				self.storage.insert(block.clone())?;
				self.storage.canonize(block.hash())?;

				// remember new best block hash
				self.best_storage_block = self.storage.as_store().best_block();

				// remove inserted block + handle possible reorganization in headers chain
				// TODO: mk, not sure if we need both of those params
				self.headers_chain.block_inserted_to_storage(block.hash(), &self.best_storage_block.hash);

				// double check
				assert_eq!(self.best_storage_block.hash, block.hash().clone());

				// all transactions from this block were accepted
				// => delete accepted transactions from verification queue and from the memory pool
				// + also remove transactions which spent outputs which have been spent by transactions from the block
				let mut memory_pool = self.memory_pool.write();
				for tx in &block.transactions {
					memory_pool.remove_by_hash(&tx.hash);
					self.verifying_transactions.remove(&tx.hash);
					for tx_input in &tx.raw.inputs {
						memory_pool.remove_by_prevout(&tx_input.previous_output);
					}
				}
				// no transactions to reverify, because we have just appended new transactions to the blockchain

				Ok(BlockInsertionResult {
					canonized_blocks_hashes: vec![block.hash().clone()],
					transactions_to_reverify: Vec::new(),
				})
			},
			// case 2: block has been added to the side branch with reorganization to this branch
			storage::BlockOrigin::SideChainBecomingCanonChain(origin) => {
				let fork = self.storage.fork(origin.clone())?;
				fork.store().insert(block.clone())?;
				fork.store().canonize(block.hash())?;
				self.storage.switch_to_fork(fork)?;

				// remember new best block hash
				self.best_storage_block = self.storage.best_block();

				// remove inserted block + handle possible reorganization in headers chain
				// TODO: mk, not sure if we need both of those params
				self.headers_chain.block_inserted_to_storage(block.hash(), &self.best_storage_block.hash);

				// all transactions from this block were accepted
				// + all transactions from previous blocks of this fork were accepted
				// => delete accepted transactions from verification queue and from the memory pool
				let this_block_transactions_hashes = block.transactions.iter().map(|tx| tx.hash.clone()).collect::<Vec<_>>();
				let mut canonized_blocks_hashes = origin.canonized_route.clone();
				let new_main_blocks_transactions_hashes = origin.canonized_route.into_iter()
					.flat_map(|block_hash| self.storage.block_transaction_hashes(block_hash.into()))
					.collect::<Vec<_>>();

				let mut memory_pool = self.memory_pool.write();
				for transaction_accepted in this_block_transactions_hashes.into_iter().chain(new_main_blocks_transactions_hashes.into_iter()) {
					memory_pool.remove_by_hash(&transaction_accepted);
					self.verifying_transactions.remove(&transaction_accepted);
				}

				// reverify all transactions from old main branch' blocks
				let old_main_blocks_transactions = origin.decanonized_route.into_iter()
					.flat_map(|block_hash| self.storage.block_transactions(block_hash.into()))
					.collect::<Vec<_>>();

				trace!(target: "sync", "insert_best_block, old_main_blocks_transactions: {:?}",
					   old_main_blocks_transactions.iter().map(|tx| tx.hash.reversed()).collect::<Vec<H256>>());

				// reverify memory pool transactions, sorted by timestamp
				let memory_pool_transactions_count = memory_pool.information().transactions_count;
				let memory_pool_transactions: Vec<IndexedTransaction> = memory_pool
					.remove_n_with_strategy(memory_pool_transactions_count, MemoryPoolOrderingStrategy::ByTimestamp)
					.into_iter()
					.map(|t| t.into())
					.collect();

				// reverify verifying transactions
				let verifying_transactions: Vec<IndexedTransaction> = self.verifying_transactions
					.iter()
					.map(|(_, t)| t.clone())
					.collect();
				self.verifying_transactions.clear();

				canonized_blocks_hashes.push(block.hash().clone());

				let result = BlockInsertionResult {
					canonized_blocks_hashes: canonized_blocks_hashes,
					// order matters: db transactions, then ordered mempool transactions, then ordered verifying transactions
					transactions_to_reverify: old_main_blocks_transactions.into_iter()
						.chain(memory_pool_transactions.into_iter())
						.chain(verifying_transactions.into_iter())
						.collect(),
				};

				trace!(target: "sync", "result: {:?}", result);

				Ok(result)
			},
			// case 3: block has been added to the side branch without reorganization to this branch
			storage::BlockOrigin::SideChain(_origin) => {
				let block_hash = block.hash().clone();
				self.storage.insert(block)?;

				// remove inserted block + handle possible reorganization in headers chain
				// TODO: mk, not sure if it's needed here at all
				self.headers_chain.block_inserted_to_storage(&block_hash, &self.best_storage_block.hash);

				// no transactions were accepted
				// no transactions to reverify
				Ok(BlockInsertionResult::default())
			},
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

	/// Get transaction state
	pub fn transaction_state(&self, hash: &H256) -> TransactionState {
		if self.verifying_transactions.contains_key(hash) {
			return TransactionState::Verifying;
		}
		if self.storage.contains_transaction(hash) {
			return TransactionState::Stored;
		}
		if self.memory_pool.read().contains(hash) {
			return TransactionState::InMemory;
		}
		TransactionState::Unknown
	}

	/// Get transactions hashes with given state
	pub fn transactions_hashes_with_state(&self, state: TransactionState) -> Vec<H256> {
		match state {
			TransactionState::InMemory => self.memory_pool.read().get_transactions_ids(),
			TransactionState::Verifying => self.verifying_transactions.keys().cloned().collect(),
			_ => panic!("wrong argument"),
		}
	}

	/// Add transaction to verifying queue
	pub fn verify_transaction(&mut self, tx: IndexedTransaction) {
		self.verifying_transactions.insert(tx.hash.clone(), tx);
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
						if entry.raw.inputs.iter().any(|i| i.previous_output.hash == hash) {
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
	pub fn transaction_by_hash(&self, hash: &H256) -> Option<IndexedTransaction> {
		self.verifying_transactions.get(hash).cloned()
			.or_else(|| self.memory_pool.read().read_by_hash(hash)
				.cloned()
				.map(|tx| IndexedTransaction::new(hash.clone(), tx)))
	}

	/// Insert transaction to memory pool
	pub fn insert_verified_transaction(&mut self, transaction: IndexedTransaction) {
		// we have verified transaction, but possibly this transaction replaces
		// existing transaction from memory pool
		// => remove previous transactions before
		let mut memory_pool = self.memory_pool.write();
		for input in &transaction.raw.inputs {
			memory_pool.remove_by_prevout(&input.previous_output);
		}
		// now insert transaction itself
		memory_pool.insert_verified(transaction, &FeeCalculator(self.storage.as_transaction_output_provider()));
	}

	/// Calculate block locator hashes for hash queue
	fn block_locator_hashes_for_queue(&self, hashes: &mut Vec<H256>) -> (BlockHeight, BlockHeight) {
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
	fn block_locator_hashes_for_storage(&self, mut index: BlockHeight, mut step: BlockHeight, hashes: &mut Vec<H256>) {
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

impl storage::TransactionProvider for Chain {
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.memory_pool.read().transaction_bytes(hash)
			.or_else(|| self.storage.transaction_bytes(hash))
	}

	fn transaction(&self, hash: &H256) -> Option<IndexedTransaction> {
		self.memory_pool.read().transaction(hash)
			.or_else(|| self.storage.transaction(hash))
	}
}

impl storage::TransactionOutputProvider for Chain {
	fn transaction_output(&self, outpoint: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		self.memory_pool.read().transaction_output(outpoint, transaction_index)
			.or_else(|| self.storage.transaction_output(outpoint, transaction_index))
	}

	fn is_spent(&self, outpoint: &OutPoint) -> bool {
		self.memory_pool.read().is_spent(outpoint)
			|| self.storage.is_spent(outpoint)
	}
}

impl storage::BlockHeaderProvider for Chain {
	fn block_header_bytes(&self, block_ref: storage::BlockRef) -> Option<Bytes> {
		use ser::serialize;
		self.block_header(block_ref).map(|h| serialize(&h.raw))
	}

	fn block_header(&self, block_ref: storage::BlockRef) -> Option<IndexedBlockHeader> {
		match block_ref {
			storage::BlockRef::Hash(hash) => self.block_header_by_hash(&hash),
			storage::BlockRef::Number(n) => self.block_header_by_number(n),
		}
	}
}

impl fmt::Debug for Information {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "[sch:{} -> req:{} -> vfy:{} -> stored: {}]", self.scheduled, self.requested, self.verifying, self.stored)
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
	extern crate test_data;

	use std::sync::Arc;
	use parking_lot::RwLock;
	use chain::{Transaction, IndexedBlockHeader};
	use db::BlockChainDatabase;
	use miner::MemoryPool;
	use network::{Network, ConsensusParams, ConsensusFork};
	use primitives::hash::H256;
	use super::{Chain, BlockState, TransactionState, BlockInsertionResult};
	use utils::HashPosition;

	#[test]
	fn chain_empty() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let db_best_block = db.best_block();
		let chain = Chain::new(db.clone(), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
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
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut chain = Chain::new(db.clone(), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));

		// add 6 blocks to scheduled queue
		let blocks = test_data::build_n_empty_blocks_from_genesis(6, 0);
		let headers: Vec<IndexedBlockHeader> = blocks.into_iter().map(|b| b.block_header.into()).collect();
		let hashes: Vec<_> = headers.iter().map(|h| h.hash.clone()).collect();
		chain.schedule_blocks_headers(headers.clone());
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
		chain.verify_block(headers[0].clone().into());
		chain.verify_block(headers[1].clone().into());
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 2 && chain.information().stored == 1);

		// mark block 0 as verified
		assert_eq!(chain.forget_block_with_state(&hashes[0], BlockState::Verifying), HashPosition::Front);
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 1);
		// insert new best block to the chain
		chain.insert_best_block(test_data::block_h1().into()).expect("Db error");
		assert!(chain.information().scheduled == 3 && chain.information().requested == 1
			&& chain.information().verifying == 1 && chain.information().stored == 2);
		assert_eq!(db.best_block().number, 1);
	}

	#[test]
	fn chain_block_locator_hashes() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		let genesis_hash = chain.best_block().hash;
		assert_eq!(chain.block_locator_hashes(), vec![genesis_hash.clone()]);

		let block1 = test_data::block_h1();
		let block1_hash = block1.hash();

		chain.insert_best_block(block1.into()).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block1_hash.clone(), genesis_hash.clone()]);

		let block2 = test_data::block_h2();
		let block2_hash = block2.hash();

		chain.insert_best_block(block2.into()).expect("Error inserting new block");
		assert_eq!(chain.block_locator_hashes(), vec![block2_hash.clone(), block1_hash.clone(), genesis_hash.clone()]);

		let blocks0 = test_data::build_n_empty_blocks_from_genesis(11, 0);
		let headers0: Vec<IndexedBlockHeader> = blocks0.into_iter().map(|b| b.block_header.into()).collect();
		let hashes0: Vec<_> = headers0.iter().map(|h| h.hash.clone()).collect();
		chain.schedule_blocks_headers(headers0.clone());
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

		let blocks1 = test_data::build_n_empty_blocks_from(6, 0, &headers0[10].raw);
		let headers1: Vec<IndexedBlockHeader> = blocks1.into_iter().map(|b| b.block_header.into()).collect();
		let hashes1: Vec<_> = headers1.iter().map(|h| h.hash.clone()).collect();
		chain.schedule_blocks_headers(headers1.clone());
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

		let blocks2 = test_data::build_n_empty_blocks_from(3, 0, &headers1[5].raw);
		let headers2: Vec<IndexedBlockHeader> = blocks2.into_iter().map(|b| b.block_header.into()).collect();
		let hashes2: Vec<_> = headers2.iter().map(|h| h.hash.clone()).collect();
		chain.schedule_blocks_headers(headers2);

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
	fn chain_transaction_state() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into(), test_data::block_h1().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		let genesis_block = test_data::genesis();
		let block2 = test_data::block_h2();
		let tx1: Transaction = test_data::TransactionBuilder::with_version(1).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_input(&test_data::genesis().transactions[0], 0).into();
		let tx1_hash = tx1.hash();
		let tx2_hash = tx2.hash();
		chain.verify_transaction(tx1.into());
		chain.insert_verified_transaction(tx2.into());

		assert_eq!(chain.transaction_state(&genesis_block.transactions[0].hash()), TransactionState::Stored);
		assert_eq!(chain.transaction_state(&block2.transactions[0].hash()), TransactionState::Unknown);
		assert_eq!(chain.transaction_state(&tx1_hash), TransactionState::Verifying);
		assert_eq!(chain.transaction_state(&tx2_hash), TransactionState::InMemory);
	}

	#[test]
	fn chain_block_transaction_is_removed_from_on_block_insert() {
		let b0 = test_data::block_builder().header().build()
			.transaction().coinbase()
				.output().value(10).build()
				.build()
			.build();
		let b1 = test_data::block_builder().header().parent(b0.hash()).build()
			.transaction().coinbase()
				.output().value(10).build()
				.build()
			.transaction()
				.input().hash(b0.transactions[0].hash()).index(0).build()
				.build()
			.build();
		let tx1 = b1.transactions[0].clone();
		let tx1_hash = tx1.hash();
		let tx2 = b1.transactions[1].clone();
		let tx2_hash = tx2.hash();

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![b0.into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.verify_transaction(tx1.into());
		chain.insert_verified_transaction(tx2.into());

		// only one transaction is in the memory pool
		assert_eq!(chain.information().transactions.transactions_count, 1);

		// when block is inserted to the database => all accepted transactions are removed from mempool && verifying queue
		chain.insert_best_block(b1.into()).expect("block accepted");

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

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.verify_transaction(test_chain.at(0).into());
		chain.verify_transaction(test_chain.at(1).into());
		chain.verify_transaction(test_chain.at(2).into());
		chain.verify_transaction(test_chain.at(3).into());

		chain.forget_verifying_transaction_with_children(&test_chain.at(0).hash());
		assert!(!chain.forget_verifying_transaction(&test_chain.at(0).hash()));
		assert!(!chain.forget_verifying_transaction(&test_chain.at(1).hash()));
		assert!(!chain.forget_verifying_transaction(&test_chain.at(2).hash()));
		assert!(chain.forget_verifying_transaction(&test_chain.at(3).hash()));
	}

	#[test]
	fn chain_transactions_hashes_with_state() {
		let input_tx1 = test_data::genesis().transactions[0].clone();
		let input_tx2 = test_data::block_h1().transactions[0].clone();
		let test_chain = &mut test_data::ChainBuilder::new();
		test_data::TransactionBuilder::with_input(&input_tx1, 0)
				.add_output(1_000).store(test_chain)						// t1
			.into_input(0).add_output(400).store(test_chain)				// t1 -> t2
			.into_input(0).add_output(300).store(test_chain)				// t1 -> t2 -> t3
			.set_input(&input_tx2, 0).set_output(400).store(test_chain);		// t4

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into(), test_data::block_h1().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.insert_verified_transaction(test_chain.at(0).into());
		chain.insert_verified_transaction(test_chain.at(1).into());
		chain.insert_verified_transaction(test_chain.at(2).into());
		chain.insert_verified_transaction(test_chain.at(3).into());

		let chain_transactions = chain.transactions_hashes_with_state(TransactionState::InMemory);
		assert!(chain_transactions.contains(&test_chain.at(0).hash()));
		assert!(chain_transactions.contains(&test_chain.at(1).hash()));
		assert!(chain_transactions.contains(&test_chain.at(2).hash()));
		assert!(chain_transactions.contains(&test_chain.at(3).hash()));
	}

	#[test]
	fn memory_pool_transactions_are_reverified_after_reorganization() {
		let b0 = test_data::block_builder()
			.header().build()
			.transaction().coinbase().output().value(100_000).build().build()
			.build();
		let b1 = test_data::block_builder().header().nonce(1).parent(b0.hash()).build().build();
		let b2 = test_data::block_builder().header().nonce(2).parent(b0.hash()).build().build();
		let b3 = test_data::block_builder().header().parent(b2.hash()).build().build();

		let input_tx = b0.transactions[0].clone();
		let tx1: Transaction = test_data::TransactionBuilder::with_version(1).set_input(&input_tx, 0).into();
		let tx1_hash = tx1.hash();
		let tx2: Transaction = test_data::TransactionBuilder::with_input(&input_tx, 0).into();
		let tx2_hash = tx2.hash();

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![b0.into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.verify_transaction(tx1.into());
		chain.insert_verified_transaction(tx2.into());

		// no reorg
		let result = chain.insert_best_block(b1.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 0);

		// no reorg
		let result = chain.insert_best_block(b2.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 0);

		// reorg
		let result = chain.insert_best_block(b3.into()).expect("no error");
		assert_eq!(result.transactions_to_reverify.len(), 2);
		assert!(result.transactions_to_reverify.iter().any(|ref tx| &tx.hash == &tx1_hash));
		assert!(result.transactions_to_reverify.iter().any(|ref tx| &tx.hash == &tx2_hash));
	}

	#[test]
	fn fork_chain_block_transaction_is_removed_from_on_block_insert() {
		let genesis = test_data::genesis();
		let input_tx = genesis.transactions[0].clone();
		let b0 = test_data::block_builder().header().parent(genesis.hash()).build().build(); // genesis -> b0
		let b1 = test_data::block_builder().header().nonce(1).parent(b0.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b0 -> b1[tx1]
		let b2 = test_data::block_builder().header().parent(b1.hash()).build()
			.transaction().output().value(20).build().build()
			.build(); // genesis -> b0 -> b1[tx1] -> b2[tx2]
		let b3 = test_data::block_builder().header().nonce(2).parent(b0.hash()).build()
			.transaction().input().hash(input_tx.hash()).index(0).build()
			.output().value(50).build().build()
			.build(); // genesis -> b0 -> b3[tx3]
		let b4 = test_data::block_builder().header().parent(b3.hash()).build()
			.transaction().input().hash(b3.transactions[0].hash()).index(0).build()
				.output().value(40).build().build()
			.build(); // genesis -> b0 -> b3[tx3] -> b4[tx4]
		let b5 = test_data::block_builder().header().parent(b4.hash()).build()
			.transaction().input().hash(b4.transactions[0].hash()).index(0).build()
				.output().value(30).build().build()
			.build(); // genesis -> b0 -> b3[tx3] -> b4[tx4] -> b5[tx5]

		let tx1 = b1.transactions[0].clone();
		let tx1_hash = tx1.hash();
		let tx2 = b2.transactions[0].clone();
		let tx2_hash = tx2.hash();
		let tx3 = b3.transactions[0].clone();
		let tx4 = b4.transactions[0].clone();
		let tx5 = b5.transactions[0].clone();

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![genesis.into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));

		chain.insert_verified_transaction(tx3.into());
		chain.insert_verified_transaction(tx4.into());
		chain.insert_verified_transaction(tx5.into());

		assert_eq!(chain.insert_best_block(b0.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b0.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b1.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b1.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b2.clone().into()).expect("block accepted"), BlockInsertionResult::with_canonized_blocks(vec![b2.hash()]));
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b3.clone().into()).expect("block accepted"), BlockInsertionResult::default());
		assert_eq!(chain.information().transactions.transactions_count, 3);
		assert_eq!(chain.insert_best_block(b4.clone().into()).expect("block accepted"), BlockInsertionResult::default());
		assert_eq!(chain.information().transactions.transactions_count, 3);
		// order matters
		let insert_result = chain.insert_best_block(b5.clone().into()).expect("block accepted");
		let transactions_to_reverify_hashes: Vec<_> = insert_result
			.transactions_to_reverify
			.into_iter()
			.map(|tx| tx.hash)
			.collect();
		assert_eq!(transactions_to_reverify_hashes, vec![tx1_hash, tx2_hash]);
		assert_eq!(insert_result.canonized_blocks_hashes, vec![b3.hash(), b4.hash(), b5.hash()]);
		assert_eq!(chain.information().transactions.transactions_count, 0); // tx3, tx4, tx5 are added to the database
	}

	#[test]
	fn double_spend_transaction_is_removed_from_memory_pool_when_output_is_spent_in_block_transaction() {
		let genesis = test_data::genesis();
		let tx0 = genesis.transactions[0].clone();
		let b0 = test_data::block_builder().header().nonce(1).parent(genesis.hash()).build()
			.transaction()
				.lock_time(1)
				.input().hash(tx0.hash()).index(0).build()
				.build()
			.build(); // genesis -> b0[tx1]
		// tx from b0 && tx2 are spending same output
		let tx2: Transaction = test_data::TransactionBuilder::with_output(20).add_input(&tx0, 0).into();

		// insert tx2 to memory pool
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.insert_verified_transaction(tx2.clone().into());
		// insert verified block with tx1
		chain.insert_best_block(b0.into()).expect("no error");
		// => tx2 is removed from memory pool, but tx3 remains
		assert_eq!(chain.information().transactions.transactions_count, 0);
	}

	#[test]
	fn update_memory_pool_transaction() {
		use self::test_data::{ChainBuilder, TransactionBuilder};

		let input_tx = test_data::genesis().transactions[0].clone();
		let data_chain = &mut ChainBuilder::new();
		TransactionBuilder::with_input(&input_tx, 0).set_output(100).store(data_chain)			// transaction0
			.reset().set_input(&data_chain.at(0), 0).add_output(20).lock().store(data_chain)	// transaction0 -> transaction1
			.reset().set_input(&data_chain.at(0), 0).add_output(30).store(data_chain);			// transaction0 -> transaction2

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut chain = Chain::new(db, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), Arc::new(RwLock::new(MemoryPool::new())));
		chain.insert_verified_transaction(data_chain.at(0).into());
		chain.insert_verified_transaction(data_chain.at(1).into());
		assert_eq!(chain.information().transactions.transactions_count, 2);
		chain.insert_verified_transaction(data_chain.at(2).into());
		assert_eq!(chain.information().transactions.transactions_count, 2); // tx was replaced
	}
}
