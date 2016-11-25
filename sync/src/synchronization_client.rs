use std::sync::Arc;
use std::cmp::{min, max};
use std::collections::{HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use parking_lot::Mutex;
use futures::{BoxFuture, Future, finished};
use futures::stream::Stream;
use tokio_core::reactor::{Handle, Interval};
use futures_cpupool::CpuPool;
use db;
use chain::{Block, BlockHeader, Transaction};
use message::types;
use message::common::{InventoryVector, InventoryType};
use primitives::hash::H256;
use synchronization_peers::Peers;
#[cfg(test)] use synchronization_peers::{Information as PeersInformation};
use synchronization_chain::{ChainRef, BlockState, TransactionState, HeadersIntersection, BlockInsertionResult};
#[cfg(test)]
use synchronization_chain::{Information as ChainInformation};
use synchronization_executor::{Task, TaskExecutor};
use orphan_blocks_pool::OrphanBlocksPool;
use orphan_transactions_pool::OrphanTransactionsPool;
use synchronization_server::ServerTaskIndex;
use synchronization_manager::{manage_synchronization_peers_blocks, manage_synchronization_peers_inventory,
	manage_unknown_orphaned_blocks, manage_orphaned_transactions, MANAGEMENT_INTERVAL_MS,
	ManagePeersConfig, ManageUnknownBlocksConfig, ManageOrphanTransactionsConfig};
use synchronization_verifier::{Verifier, VerificationSink};
use hash_queue::HashPosition;
use miner::transaction_fee_rate;
use time;
use std::time::Duration;

#[cfg_attr(feature="cargo-clippy", allow(doc_markdown))]
///! TODO: update with headers-first corrections
///!
///! Blocks synchronization process:
///!
///! When new peer is connected:
///! 1) send `getheaders` message with full block locator hashes (see `LocalNode`)
///!
///! on_new_blocks_headers: When `headers` message is received from peer:
///! 1) queue_intersection = intersect(queue, inventory)
///! 2) if !queue_intersection.is_empty(): ===> responded with blocks within sync window
///! 2.1) remember peer as useful
///! 2.2) inventory_rest = inventory - queue_intersection
///! 2.3) if inventory_rest.is_empty(): ===> no new unknown blocks in inventory
///! 2.3.1) stop (2.3)
///! 2.4) if !inventory_rest.is_empty(): ===> has new unknown blocks in inventory
///! 2.4.1) queue_rest = queue after intersection
///! 2.4.2) if queue_rest.is_empty(): ===> has new unknown blocks in inventory, no fork
///! 2.4.2.1) scheduled_blocks.append(inventory_rest)
///! 2.4.2.2) stop (2.4.2)
///! 2.4.3) if !queue_rest.is_empty(): ===> has new unknown blocks in inventory, fork
///! 2.4.3.1) scheduled_blocks.append(inventory_rest)
///! 2.4.3.2) stop (2.4.3)
///! 2.4.3) stop (2.4)
///! 2.5) stop (2)
///! 3) if queue_intersection.is_empty(): ===> responded with out-of-sync-window blocks
///! 3.1) last_known_block = inventory.last(b => b.is_known())
///! 3.2) if last_known_block == None: ===> we know nothing about these blocks & we haven't asked for these
///! 3.2.1) if !synchronizing => remember peer as useful + ask for blocks
///! 3.2.1) if synchronizing => peer will be excluded later by management thread
///! 3.2.2) stop (3.2)
///! 3.3) if last_known_block == last(inventory): ===> responded with all-known-blocks
///! 3.3.1) if syncing, remember peer as useful (possibly had failures before && have been excluded from sync)
///! 3.3.2) stop (3.3)
///! 3.4) if last_known_block in the middle of inventory: ===> responded with forked blocks
///! 3.4.1) remember peer as useful
///! 3.4.2) inventory_rest = inventory after last_known_block
///! 3.4.3) scheduled_blocks.append(inventory_rest)
///! 3.4.4) stop (3.4)
///! 3.5) stop (3)
///!
///! on_peer_block: After receiving `block` message:
///! 1) if block_state(block) in (Verifying, Stored): ===> late delivery
///! 1.1) remember peer as useful
///! 1.2) stop (1)
///! 2) if block_state(block) in (Scheduled, Requested): ===> future/on-time delivery
///! 2.1) remember peer as useful
///! 2.2) if block_state(block.parent) in (Verifying, Stored): ===> we can proceed with verification
///! 2.2.1) remove block from current queue (Verifying || Stored)
///! 2.2.2) append block to the verification queue
///! 2.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 2.2.4) try to verify orphan blocks
///! 2.2.5) stop (2.2)
///! 2.3) if block_state(block.parent) in (Requested, Scheduled): ===> we have found an orphan block
///! 2.3.1) remove block from current queue (Verifying || Stored)
///! 2.3.2) append block to the orphans
///! 2.3.3) stop (2.3)
///! 2.4) if block_state(block.parent) == Unknown: ===> bad block found
///! 2.4.1) remove block from current queue (Verifying || Stored)
///! 2.4.2) stop (2.4)
///! 2.5) stop (2)
///! 3) if block_state(block) == Unknown: ===> maybe we are on-top of chain && new block is announced?
///! 3.1) if block_state(block.parent_hash) == Unknown: ===> we do not know parent
///! 3.1.1) ignore this block
///! 3.1.2) stop (3.1)
///! 3.2) if block_state(block.parent_hash) in (Verifying, Stored): ===> fork found, can verify
///! 3.2.1) ask peer for best inventory (after this block)
///! 3.2.2) append block to verifying queue
///! 3.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 3.2.4) stop (3.2)
///! 3.3) if block_state(block.parent_hash) in (Requested, Scheduled): ===> fork found, add as orphan
///! 3.3.1) ask peer for best inventory (after this block)
///! 3.3.2) append block to orphan
///! 3.3.3) stop (3.3)
///! 3.4) stop (2)
///! + if no blocks left in scheduled + requested queue => we are saturated => ask all peers for inventory & forget
///!
///! execute_synchronization_tasks: After receiving `headers`/`inventory` message OR receiving `block` message OR when management thread schedules tasks:
///! 1) if there are blocks in `scheduled` queue AND we can fit more blocks into memory: ===> ask for blocks
///! 1.1) select idle peers
///! 1.2) for each idle peer: query chunk of blocks from `scheduled` queue
///! 1.3) move requested blocks from `scheduled` to `requested` queue
///! 1.4) mark idle peers as active
///! 1.5) stop (1)
///! 2) if `scheduled` queue is not yet saturated: ===> ask for new blocks hashes
///! 2.1) for each idle peer: send shortened `getblocks` message
///! 2.2) 'forget' idle peers => they will be added again if respond with inventory
///! 2.3) stop (2)
///!
///! manage_synchronization_peers: When management thread awakes:
///! 1) for peer in active_peers.where(p => now() - p.last_request_time() > failure_interval):
///! 1.1) return all peer' tasks to the tasks pool + TODO: filter tasks (if we have requested some hash several times from several peers && they haven't responded => drop this hash + reset sync???)
///! 1.2) increase # of failures for this peer
///! 1.3) if # of failures > max_failures: ===> super-bad peer
///! 1.3.1) forget peer
///! 1.3.3) stop (1.3)
///! 1.4) if # of failures <= max_failures: ===> bad peer
///! 1.4.1) move peer to idle pool
///! 1.4.2) stop (1.4)
///! 2) schedule tasks from pool (if any)
///!
///! on_block_verification_success: When verification completes scuccessfully:
///! 1) if block_state(block) != Verifying: ===> parent verification failed
///! 1.1) stop (1)
///! 2) remove from verifying queue
///! 3) insert to the db
///!
///! on_block_verification_error: When verification completes with an error:
///! 1) remove block from verification queue
///! 2) remove all known children from all queues [so that new `block` messages will be ignored in on_peer_block.3.1.1]
///!

/// Approximate maximal number of blocks hashes in scheduled queue.
const MAX_SCHEDULED_HASHES: u32 = 4 * 1024;
/// Approximate maximal number of blocks hashes in requested queue.
const MAX_REQUESTED_BLOCKS: u32 = 256;
/// Approximate maximal number of blocks in verifying queue.
const MAX_VERIFYING_BLOCKS: u32 = 256;
/// Minimum number of blocks to request from peer
const MIN_BLOCKS_IN_REQUEST: u32 = 32;
/// Maximum number of blocks to request from peer
const MAX_BLOCKS_IN_REQUEST: u32 = 128;

/// Synchronization state
#[derive(Debug, Clone, Copy)]
pub enum State {
	/// We know that there are > 1 unknown blocks, unknown to us in the blockchain
	Synchronizing(f64, u32),
	/// There is only one unknown block in the blockchain
	NearlySaturated,
	/// We have downloaded all blocks of the blockchain of which we have ever heard
	Saturated,
}

/// Information on current synchronization state.
#[cfg(test)]
#[derive(Debug)]
pub struct Information {
	/// Current synchronization state.
	pub state: State,
	/// Information on synchronization peers.
	pub peers: PeersInformation,
	/// Current synchronization chain inormation.
	pub chain: ChainInformation,
	/// Number of currently orphaned blocks.
	pub orphaned_blocks: usize,
	/// Number of currently orphaned transactions.
	pub orphaned_transactions: usize,
}

/// Synchronization client trait
pub trait Client : Send + 'static {
	fn best_block(&self) -> db::BestBlock;
	fn state(&self) -> State;
	fn filter_getdata_inventory(&mut self, peer_index: usize, inventory: Vec<InventoryVector>) -> FilteredInventory;
	fn on_peer_connected(&mut self, peer_index: usize);
	fn on_new_blocks_inventory(&mut self, peer_index: usize, blocks_hashes: Vec<H256>);
	fn on_new_transactions_inventory(&mut self, peer_index: usize, transactions_hashes: Vec<H256>);
	fn on_new_blocks_headers(&mut self, peer_index: usize, blocks_headers: Vec<BlockHeader>);
	fn on_peer_blocks_notfound(&mut self, peer_index: usize, blocks_hashes: Vec<H256>);
	fn on_peer_block(&mut self, peer_index: usize, block: Block);
	fn on_peer_transaction(&mut self, peer_index: usize, transaction: Transaction);
	fn on_peer_filterload(&mut self, peer_index: usize, message: &types::FilterLoad);
	fn on_peer_filteradd(&mut self, peer_index: usize, message: &types::FilterAdd);
	fn on_peer_filterclear(&mut self, peer_index: usize);
	fn on_peer_sendheaders(&mut self, peer_index: usize);
	fn on_peer_feefilter(&mut self, peer_index: usize, message: &types::FeeFilter);
	fn on_peer_disconnected(&mut self, peer_index: usize);
	fn after_peer_nearly_blocks_verified(&mut self, peer_index: usize, future: BoxFuture<(), ()>);
}

/// Synchronization client trait
pub trait ClientCore : VerificationSink {
	fn best_block(&self) -> db::BestBlock;
	fn state(&self) -> State;
	fn filter_getdata_inventory(&mut self, peer_index: usize, inventory: Vec<InventoryVector>) -> FilteredInventory;
	fn on_peer_connected(&mut self, peer_index: usize);
	fn on_new_blocks_inventory(&mut self, peer_index: usize, blocks_hashes: Vec<H256>);
	fn on_new_transactions_inventory(&mut self, peer_index: usize, transactions_hashes: Vec<H256>);
	fn on_new_blocks_headers(&mut self, peer_index: usize, blocks_headers: Vec<BlockHeader>);
	fn on_peer_blocks_notfound(&mut self, peer_index: usize, blocks_hashes: Vec<H256>);
	fn on_peer_block(&mut self, peer_index: usize, block: Block) -> Option<VecDeque<(H256, Block)>>;
	fn on_peer_transaction(&mut self, peer_index: usize, transaction: Transaction) -> Option<VecDeque<(H256, Transaction)>>;
	fn on_peer_filterload(&mut self, peer_index: usize, message: &types::FilterLoad);
	fn on_peer_filteradd(&mut self, peer_index: usize, message: &types::FilterAdd);
	fn on_peer_filterclear(&mut self, peer_index: usize);
	fn on_peer_sendheaders(&mut self, peer_index: usize);
	fn on_peer_feefilter(&mut self, peer_index: usize, message: &types::FeeFilter);
	fn on_peer_disconnected(&mut self, peer_index: usize);
	fn after_peer_nearly_blocks_verified(&mut self, peer_index: usize, future: BoxFuture<(), ()>);
	fn execute_synchronization_tasks(&mut self, forced_blocks_requests: Option<Vec<H256>>);
	fn try_switch_to_saturated_state(&mut self) -> bool;
}


/// Synchronization client configuration options.
#[derive(Debug)]
pub struct Config {
	/// Number of threads to allocate in synchronization CpuPool.
	pub threads_num: usize,
}

/// Filtered `getdata` inventory.
#[derive(Debug, PartialEq)]
pub struct FilteredInventory {
	/// Merkleblock messages + transactions to send after
	pub filtered: Vec<(types::MerkleBlock, Vec<(H256, Transaction)>)>,
	/// Rest of inventory with MessageTx, MessageBlock, MessageCompactBlock inventory types
	pub unfiltered: Vec<InventoryVector>,
	/// Items that were supposed to be filtered, but we know nothing about these
	pub notfound: Vec<InventoryVector>,
}

/// Synchronization client facade
pub struct SynchronizationClient<T: TaskExecutor, U: Verifier> {
	/// Client core
	core: Arc<Mutex<SynchronizationClientCore<T>>>,
	/// Verifier
	verifier: U,
}

/// Synchronization client.
pub struct SynchronizationClientCore<T: TaskExecutor> {
	/// Synchronization state.
	state: State,
	/// Cpu pool.
	pool: CpuPool,
	/// Sync management worker.
	management_worker: Option<BoxFuture<(), ()>>,
	/// Synchronization peers.
	peers: Peers,
	/// Task executor.
	executor: Arc<Mutex<T>>,
	/// Chain reference.
	chain: ChainRef,
	/// Orphaned blocks pool.
	orphaned_blocks_pool: OrphanBlocksPool,
	/// Orphaned transactions pool.
	orphaned_transactions_pool: OrphanTransactionsPool,
	/// Verifying blocks by peer
	verifying_blocks_by_peer: HashMap<H256, usize>,
	/// Verifying blocks futures
	verifying_blocks_futures: HashMap<usize, (HashSet<H256>, Vec<BoxFuture<(), ()>>)>,
}

impl Config {
	pub fn new() -> Self {
		Config {
			threads_num: 4,
		}
	}
}

impl FilteredInventory {
	#[cfg(test)]
	pub fn with_unfiltered(unfiltered: Vec<InventoryVector>) -> Self {
		FilteredInventory {
			filtered: Vec::new(),
			unfiltered: unfiltered,
			notfound: Vec::new(),
		}
	}

	#[cfg(test)]
	pub fn with_notfound(notfound: Vec<InventoryVector>) -> Self {
		FilteredInventory {
			filtered: Vec::new(),
			unfiltered: Vec::new(),
			notfound: notfound,
		}
	}
}

impl State {
	pub fn is_saturated(&self) -> bool {
		match *self {
			State::Saturated => true,
			_ => false,
		}
	}

	pub fn is_synchronizing(&self) -> bool {
		match *self {
			State::Synchronizing(_, _) => true,
			_ => false,
		}
	}

	pub fn is_nearly_saturated(&self) -> bool {
		match *self {
			State::NearlySaturated => true,
			_ => false,
		}
	}
}

impl<T, U> Client for SynchronizationClient<T, U> where T: TaskExecutor, U: Verifier {
	fn best_block(&self) -> db::BestBlock {
		self.core.lock().best_block()
	}

	fn state(&self) -> State {
		self.core.lock().state()
	}

	fn filter_getdata_inventory(&mut self, peer_index: usize, inventory: Vec<InventoryVector>) -> FilteredInventory {
		self.core.lock().filter_getdata_inventory(peer_index, inventory)
	}

	fn on_peer_connected(&mut self, peer_index: usize) {
		self.core.lock().on_peer_connected(peer_index);
	}

	fn on_new_blocks_inventory(&mut self, peer_index: usize, blocks_hashes: Vec<H256>) {
		self.core.lock().on_new_blocks_inventory(peer_index, blocks_hashes)
	}

	fn on_new_transactions_inventory(&mut self, peer_index: usize, transactions_hashes: Vec<H256>) {
		self.core.lock().on_new_transactions_inventory(peer_index, transactions_hashes)
	}

	fn on_new_blocks_headers(&mut self, peer_index: usize, blocks_headers: Vec<BlockHeader>) {
		self.core.lock().on_new_blocks_headers(peer_index, blocks_headers);
	}

	fn on_peer_blocks_notfound(&mut self, peer_index: usize, blocks_hashes: Vec<H256>) {
		self.core.lock().on_peer_blocks_notfound(peer_index, blocks_hashes);
	}

	fn on_peer_block(&mut self, peer_index: usize, block: Block) {
		let blocks_to_verify = { self.core.lock().on_peer_block(peer_index, block) };

		// verify selected blocks
		if let Some(mut blocks_to_verify) = blocks_to_verify {
			while let Some((_, block)) = blocks_to_verify.pop_front() {
				self.verifier.verify_block(block);
			}
		}

		// try to switch to saturated state OR execute sync tasks
		{
			let mut client = self.core.lock();
			if !client.try_switch_to_saturated_state() {
				client.execute_synchronization_tasks(None);
			}
		}
	}

	fn on_peer_transaction(&mut self, peer_index: usize, transaction: Transaction) {
		let transactions_to_verify = { self.core.lock().on_peer_transaction(peer_index, transaction) };

		if let Some(mut transactions_to_verify) = transactions_to_verify {
			while let Some((_, tx)) = transactions_to_verify.pop_front() {
				self.verifier.verify_transaction(tx);
			}
		}
	}

	fn on_peer_filterload(&mut self, peer_index: usize, message: &types::FilterLoad) {
		self.core.lock().on_peer_filterload(peer_index, message);
	}

	fn on_peer_filteradd(&mut self, peer_index: usize, message: &types::FilterAdd) {
		self.core.lock().on_peer_filteradd(peer_index, message);
	}

	fn on_peer_filterclear(&mut self, peer_index: usize) {
		self.core.lock().on_peer_filterclear(peer_index);
	}

	fn on_peer_sendheaders(&mut self, peer_index: usize) {
		self.core.lock().on_peer_sendheaders(peer_index);
	}

	fn on_peer_feefilter(&mut self, peer_index: usize, message: &types::FeeFilter) {
		self.core.lock().on_peer_feefilter(peer_index, message);
	}

	fn on_peer_disconnected(&mut self, peer_index: usize) {
		self.core.lock().on_peer_disconnected(peer_index);
	}

	fn after_peer_nearly_blocks_verified(&mut self, peer_index: usize, future: BoxFuture<(), ()>) {
		self.core.lock().after_peer_nearly_blocks_verified(peer_index, future);
	}
}

impl<T, U> SynchronizationClient<T, U> where T: TaskExecutor, U: Verifier {
	/// Create new synchronization client
	pub fn new(core: Arc<Mutex<SynchronizationClientCore<T>>>, verifier: U) -> Arc<Mutex<Self>> {
		Arc::new(Mutex::new(
			SynchronizationClient {
				core: core,
				verifier: verifier,
			}
		))
	}

	/// Get information on current synchronization state.
	#[cfg(test)]
	pub fn information(&self) -> Information {
		self.core.lock().information()
	}
}

impl<T> ClientCore for SynchronizationClientCore<T> where T: TaskExecutor {
	/// Get best known block
	fn best_block(&self) -> db::BestBlock {
		self.chain.read().best_block()
	}

	/// Get synchronization state
	fn state(&self) -> State {
		self.state
	}

	/// Filter inventory from `getdata` message for given peer
	fn filter_getdata_inventory(&mut self, peer_index: usize, inventory: Vec<InventoryVector>) -> FilteredInventory {
		let chain = self.chain.read();
		let mut filter = self.peers.filter_mut(peer_index);
		let mut filtered: Vec<(types::MerkleBlock, Vec<(H256, Transaction)>)> = Vec::new();
		let mut unfiltered: Vec<InventoryVector> = Vec::new();
		let mut notfound: Vec<InventoryVector> = Vec::new();

		for item in inventory {
			match item.inv_type {
				// if peer asks for filtered block => we should:
				// 1) check if block has any transactions, matching connection bloom filter
				// 2) build && send `merkleblock` message for this block
				// 3) send all matching transactions after this block
				InventoryType::MessageFilteredBlock => {
					match chain.storage().block(db::BlockRef::Hash(item.hash.clone())) {
						None => notfound.push(item),
						Some(block) => match filter.build_merkle_block(block) {
							None => notfound.push(item),
							Some(merkleblock) => filtered.push((merkleblock.merkleblock, merkleblock.matching_transactions)),
						}
					}
				},
				// these will be filtered (found/not found) in sync server
				_ => unfiltered.push(item),
			}
		}

		FilteredInventory {
			filtered: filtered,
			unfiltered: unfiltered,
			notfound: notfound,
		}
	}

	/// Called when new peer connection is established
	fn on_peer_connected(&mut self, peer_index: usize) {
		// unuseful until respond with headers message
		self.peers.unuseful_peer(peer_index);
	}

	/// Try to queue synchronization of unknown blocks when new inventory is received.
	fn on_new_blocks_inventory(&mut self, peer_index: usize, blocks_hashes: Vec<H256>) {
		// we use headers-first synchronization
		// we know nothing about these blocks
		// =>

		// if we are in synchronization state, we will ignore this message
		if self.state.is_synchronizing() {
			return;
		}

		// else => request all unknown blocks
		let unknown_blocks_hashes: Vec<_> = {
			let chain = self.chain.read();
			blocks_hashes.into_iter()
				.filter(|h| chain.block_state(h) == BlockState::Unknown)
				.filter(|h| !self.orphaned_blocks_pool.contains_unknown_block(h))
				.collect()
		};

		if !unknown_blocks_hashes.is_empty() {
			let mut executor = self.executor.lock();
			executor.execute(Task::RequestBlocks(peer_index, unknown_blocks_hashes));
		}
	}

	/// Add new transactions to the memory pool
	fn on_new_transactions_inventory(&mut self, peer_index: usize, transactions_hashes: Vec<H256>) {
		// if we are in synchronization state, we will ignore this message
		if self.state.is_synchronizing() {
			return;
		}

		// else => request all unknown transactions
		let unknown_transactions_hashes: Vec<_> = {
			let chain = self.chain.read();
			transactions_hashes.into_iter()
				.filter(|h| chain.transaction_state(h) == TransactionState::Unknown)
				.collect()
		};

		if !unknown_transactions_hashes.is_empty() {
			let mut executor = self.executor.lock();
			executor.execute(Task::RequestTransactions(peer_index, unknown_transactions_hashes));
		}
	}

	/// Try to queue synchronization of unknown blocks when blocks headers are received.
	fn on_new_blocks_headers(&mut self, peer_index: usize, blocks_headers: Vec<BlockHeader>) {
		let blocks_hashes = {
			// we can't process headers message if it has no link to our headers
			let header0 = &blocks_headers[0];
			let unknown_state = self.chain.read().block_state(&header0.previous_header_hash) == BlockState::Unknown;
			if unknown_state {
				warn!(
					target: "sync",
					"Previous header of the first header from peer#{} `headers` message is unknown. First: {:?}. Previous: {:?}",
					peer_index,
					header0.hash().to_reversed_str(),
					header0.previous_header_hash.to_reversed_str()
				);
				return;
			}

			// TODO: add full blocks headers validation here
			// validate blocks headers before scheduling
			let mut blocks_hashes: Vec<H256> = Vec::with_capacity(blocks_headers.len());
			let mut prev_block_hash = header0.previous_header_hash.clone();
			for block_header in &blocks_headers {
				let block_header_hash = block_header.hash();
				if block_header.previous_header_hash != prev_block_hash {
					warn!(target: "sync", "Neighbour headers in peer#{} `headers` message are unlinked: Prev: {:?}, PrevLink: {:?}, Curr: {:?}", peer_index, prev_block_hash, block_header.previous_header_hash, block_header_hash);
					return;
				}

				blocks_hashes.push(block_header_hash.clone());
				prev_block_hash = block_header_hash;
			}

			blocks_hashes
		};

		// update peers to select next tasks
		self.peers.on_inventory_received(peer_index);

		// now insert unknown blocks to the queue
		self.process_new_blocks_headers(peer_index, blocks_hashes, blocks_headers);
		self.execute_synchronization_tasks(None);
	}

	/// When peer has no blocks
	fn on_peer_blocks_notfound(&mut self, peer_index: usize, blocks_hashes: Vec<H256>) {
		if let Some(requested_blocks) = self.peers.get_blocks_tasks(peer_index) {
			// check if peer has responded with notfound to requested blocks
			let notfound_blocks: HashSet<H256> = blocks_hashes.into_iter().collect();
			if requested_blocks.intersection(&notfound_blocks).nth(0).is_none() {
				// if notfound some other blocks => just ignore the message
				return;
			}

			// for now, let's exclude peer from synchronization - we are relying on full nodes for synchronization
			let removed_tasks = self.peers.reset_blocks_tasks(peer_index);
			self.peers.unuseful_peer(peer_index);

			// if peer has had some blocks tasks, rerequest these blocks
			self.execute_synchronization_tasks(Some(removed_tasks));
		}
	}

	/// Process new block.
	fn on_peer_block(&mut self, peer_index: usize, block: Block) -> Option<VecDeque<(H256, Block)>> {
		let block_hash = block.hash();

		// update peers to select next tasks
		self.peers.on_block_received(peer_index, &block_hash);

		self.process_peer_block(peer_index, block_hash, block)
	}

	/// Process new transaction.
	fn on_peer_transaction(&mut self, peer_index: usize, transaction: Transaction) -> Option<VecDeque<(H256, Transaction)>> {
		let transaction_hash = transaction.hash();

		// remember that peer has this transaction
		self.peers.on_transaction_received(peer_index, &transaction_hash);

		self.process_peer_transaction(Some(peer_index), transaction_hash, transaction)
	}

	/// Peer wants to set bloom filter for the connection
	fn on_peer_filterload(&mut self, peer_index: usize, message: &types::FilterLoad) {
		if self.peers.is_known_peer(peer_index) {
			self.peers.filter_mut(peer_index).load(message);
		}
	}

	/// Peer wants to update bloom filter for the connection
	fn on_peer_filteradd(&mut self, peer_index: usize, message: &types::FilterAdd) {
		if self.peers.is_known_peer(peer_index) {
			self.peers.filter_mut(peer_index).add(message);
		}
	}

	/// Peer wants to remove bloom filter for the connection
	fn on_peer_filterclear(&mut self, peer_index: usize) {
		if self.peers.is_known_peer(peer_index) {
			self.peers.filter_mut(peer_index).clear();
		}
	}

	/// Peer wants to get blocks headers instead of blocks hashes when announcing new blocks
	fn on_peer_sendheaders(&mut self, peer_index: usize) {
		if self.peers.is_known_peer(peer_index) {
			self.peers.on_peer_sendheaders(peer_index);
		}
	}

	/// Peer wants to limit transaction announcing by transaction fee
	fn on_peer_feefilter(&mut self, peer_index: usize, message: &types::FeeFilter) {
		if self.peers.is_known_peer(peer_index) {
			self.peers.on_peer_feefilter(peer_index, message.fee_rate);
		}
	}

	/// Peer disconnected.
	fn on_peer_disconnected(&mut self, peer_index: usize) {
		// when last peer is disconnected, reset, but let verifying blocks be verified
		let peer_tasks = self.peers.on_peer_disconnected(peer_index);
		if !self.peers.has_any_useful() {
			self.switch_to_saturated_state();
		} else if peer_tasks.is_some() {
			self.execute_synchronization_tasks(peer_tasks);
		}
	}

	/// Execute after last block from this peer in NearlySaturated state is verified.
	/// If there are no verifying blocks from this peer or we are not in the NearlySaturated state => execute immediately.
	fn after_peer_nearly_blocks_verified(&mut self, peer_index: usize, future: BoxFuture<(), ()>) {
		// if we are currently synchronizing => no need to wait
		if self.state.is_synchronizing() {
			future.wait().expect("no-error future");
			return;
		}

		// we have to wait until all previous peer requests are server
		match self.verifying_blocks_futures.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				entry.get_mut().1.push(future);
			},
			_ => future.wait().expect("no-error future"),
		}
	}

	/// Schedule new synchronization tasks, if any.
	fn execute_synchronization_tasks(&mut self, forced_blocks_requests: Option<Vec<H256>>) {
		let mut tasks: Vec<Task> = Vec::new();

		// display information if processed many blocks || enough time has passed since sync start
		self.print_synchronization_information();

		// if some blocks requests are forced => we should ask peers even if there are no idle peers
		if let Some(forced_blocks_requests) = forced_blocks_requests {
			let useful_peers = self.peers.useful_peers();
			// if we have to request blocks && there are no useful peers at all => switch to saturated state
			if useful_peers.is_empty() {
				warn!(target: "sync", "Last peer was marked as non-useful. Moving to saturated state.");
				self.switch_to_saturated_state();
				return;
			}

			let forced_tasks = self.prepare_blocks_requests_tasks(useful_peers, forced_blocks_requests);
			tasks.extend(forced_tasks);
		}

		let mut blocks_requests: Option<Vec<H256>> = None;
		let blocks_idle_peers = self.peers.idle_peers_for_blocks();
		{
			// check if we can query some blocks hashes
			let inventory_idle_peers = self.peers.idle_peers_for_inventory();
			if !inventory_idle_peers.is_empty() {
				let scheduled_hashes_len = { self.chain.read().length_of_blocks_state(BlockState::Scheduled) };
				if scheduled_hashes_len < MAX_SCHEDULED_HASHES {
					for inventory_peer in &inventory_idle_peers {
						self.peers.on_inventory_requested(*inventory_peer);
					}

					let inventory_tasks = inventory_idle_peers.into_iter().map(Task::RequestBlocksHeaders);
					tasks.extend(inventory_tasks);
				}
			}

			// check if we can move some blocks from scheduled to requested queue
			let blocks_idle_peers_len = blocks_idle_peers.len() as u32;
			if blocks_idle_peers_len != 0 {
				let mut chain = self.chain.write();
				let scheduled_hashes_len = chain.length_of_blocks_state(BlockState::Scheduled);
				let requested_hashes_len = chain.length_of_blocks_state(BlockState::Requested);
				let verifying_hashes_len = chain.length_of_blocks_state(BlockState::Verifying);
				if requested_hashes_len + verifying_hashes_len < MAX_REQUESTED_BLOCKS + MAX_VERIFYING_BLOCKS && scheduled_hashes_len != 0 {
					let chunk_size = min(MAX_BLOCKS_IN_REQUEST, max(scheduled_hashes_len / blocks_idle_peers_len, MIN_BLOCKS_IN_REQUEST));
					let hashes_to_request_len = chunk_size * blocks_idle_peers_len;
					let hashes_to_request = chain.request_blocks_hashes(hashes_to_request_len);
					blocks_requests = Some(hashes_to_request);
				}
			}
		}

		// append blocks requests tasks
		if let Some(blocks_requests) = blocks_requests {
			tasks.extend(self.prepare_blocks_requests_tasks(blocks_idle_peers, blocks_requests));
		}

		// execute synchronization tasks
		for task in tasks {
			self.executor.lock().execute(task);
		}
	}

	fn try_switch_to_saturated_state(&mut self) -> bool {
		let switch_to_saturated = {
			let chain = self.chain.read();

			// requested block is received => move to saturated state if there are no more blocks
			chain.length_of_blocks_state(BlockState::Scheduled) == 0
				&& chain.length_of_blocks_state(BlockState::Requested) == 0
		};

		if switch_to_saturated {
			self.switch_to_saturated_state();
		}

		switch_to_saturated
	}
}

impl<T> VerificationSink for SynchronizationClientCore<T> where T: TaskExecutor {
	/// Process successful block verification
	fn on_block_verification_success(&mut self, block: Block) {
		let hash = block.hash();
		// insert block to the storage
		match {
			let mut chain = self.chain.write();

			// remove block from verification queue
			// header is removed in `insert_best_block` call
			// or it is removed earlier, when block was removed from the verifying queue
			if chain.forget_block_with_state_leave_header(&hash, BlockState::Verifying) != HashPosition::Missing {
				// block was in verification queue => insert to storage
				chain.insert_best_block(hash.clone(), &block)
			} else {
				Ok(BlockInsertionResult::default())
			}
		} {
			Ok(insert_result) => {
				// awake threads, waiting for this block insertion
				self.awake_waiting_threads(&hash);

				// continue with synchronization
				self.execute_synchronization_tasks(None);

				// relay block to our peers
				// TODO:
				// SPV clients that wish to use Bloom filtering would normally set version.fRelay to false in the version message,
				// then set a filter based on their wallet (or a subset of it, if they are overlapping different peers).
				// Being able to opt-out of inv messages until the filter is set prevents a client being flooded with traffic in
				// the brief window of time between finishing version handshaking and setting the filter.
				if self.state.is_saturated() || self.state.is_nearly_saturated() {
					self.relay_new_blocks(insert_result.canonized_blocks_hashes);
				}

				// deal with block transactions
				for (hash, tx) in insert_result.transactions_to_reverify {
					// TODO: transactions from this blocks will be relayed. Do we need this?
					self.process_peer_transaction(None, hash, tx);
				}
			},
			Err(db::Error::Consistency(e)) => {
				// process as verification error
				self.on_block_verification_error(&format!("{:?}", db::Error::Consistency(e)), &hash);
			},
			Err(e) => {
				// process as irrecoverable failure
				panic!("Block {:?} insertion failed with error {:?}", hash, e);
			}
		}
	}

	/// Process failed block verification
	fn on_block_verification_error(&mut self, err: &str, hash: &H256) {
		warn!(target: "sync", "Block {:?} verification failed with error {:?}", hash.to_reversed_str(), err);

		{
			let mut chain = self.chain.write();

			// forget for this block and all its children
			// headers are also removed as they all are invalid
			chain.forget_block_with_children(hash);
		}

		// awake threads, waiting for this block insertion
		self.awake_waiting_threads(hash);

		// start new tasks
		self.execute_synchronization_tasks(None);
	}

	/// Process successful transaction verification
	fn on_transaction_verification_success(&mut self, transaction: Transaction) {
		let hash = transaction.hash();

		let transaction_fee_rate = {
			// insert transaction to the memory pool
			let mut chain = self.chain.write();

			// remove transaction from verification queue
			// if it is not in the queue => it was removed due to error or reorganization
			if !chain.forget_verifying_transaction(&hash) {
				return;
			}

			// transaction was in verification queue => insert to memory pool
			chain.insert_verified_transaction(transaction.clone());

			// calculate transaction fee rate
			transaction_fee_rate(&*chain, &transaction)
		};

		// relay transaction to peers
		self.relay_new_transactions(vec![(hash, &transaction, transaction_fee_rate)]);
	}

	/// Process failed transaction verification
	fn on_transaction_verification_error(&mut self, err: &str, hash: &H256) {
		warn!(target: "sync", "Transaction {:?} verification failed with error {:?}", hash.to_reversed_str(), err);

		{
			let mut chain = self.chain.write();

			// forget for this transaction and all its children
			chain.forget_verifying_transaction_with_children(hash);
		}
	}
}

impl<T> SynchronizationClientCore<T> where T: TaskExecutor {
	/// Create new synchronization client core
	pub fn new(config: Config, handle: &Handle, executor: Arc<Mutex<T>>, chain: ChainRef) -> Arc<Mutex<Self>> {
		let sync = Arc::new(Mutex::new(
			SynchronizationClientCore {
				state: State::Saturated,
				peers: Peers::new(),
				pool: CpuPool::new(config.threads_num),
				management_worker: None,
				executor: executor,
				chain: chain.clone(),
				orphaned_blocks_pool: OrphanBlocksPool::new(),
				orphaned_transactions_pool: OrphanTransactionsPool::new(),
				verifying_blocks_by_peer: HashMap::new(),
				verifying_blocks_futures: HashMap::new(),
			}
		));

		// TODO: start management worker only when synchronization is started
		//       currently impossible because there is no way to call Interval::new with Remote && Handle is not-Send
		{
			let peers_config = ManagePeersConfig::default();
			let unknown_config = ManageUnknownBlocksConfig::default();
			let orphan_config = ManageOrphanTransactionsConfig::default();
			let csync = Arc::downgrade(&sync);
			let mut sync = sync.lock();
			let management_worker = Interval::new(Duration::from_millis(MANAGEMENT_INTERVAL_MS), handle)
				.expect("Failed to create interval")
				.and_then(move |_| {
					let client = match csync.upgrade() {
						Some(client) => client,
						None => return Ok(()),
					};
					let mut client = client.lock();
					client.print_synchronization_information();
					if client.state.is_synchronizing() || client.state.is_nearly_saturated() {
						let blocks_to_request = manage_synchronization_peers_blocks(&peers_config, &mut client.peers);
						client.execute_synchronization_tasks(blocks_to_request);

						manage_synchronization_peers_inventory(&peers_config, &mut client.peers);
						manage_orphaned_transactions(&orphan_config, &mut client.orphaned_transactions_pool);
						if let Some(orphans_to_remove) = manage_unknown_orphaned_blocks(&unknown_config, &mut client.orphaned_blocks_pool) {
							let mut chain = client.chain.write();
							for orphan_to_remove in orphans_to_remove {
								chain.forget_block(&orphan_to_remove);
							}
						}
					}
					Ok(())
				})
				.for_each(|_| Ok(()))
				.then(|_| finished::<(), ()>(()))
				.boxed();
			sync.management_worker = Some(sync.pool.spawn(management_worker).boxed());
		}

		sync
	}

	/// Get information on current synchronization state.
	#[cfg(test)]
	pub fn information(&self) -> Information {
		Information {
			state: self.state,
			peers: self.peers.information(),
			chain: self.chain.read().information(),
			orphaned_blocks: self.orphaned_blocks_pool.len(),
			orphaned_transactions: self.orphaned_transactions_pool.len(),
		}
	}

	/// Relay new blocks
	fn relay_new_blocks(&mut self, new_blocks_hashes: Vec<H256>) {
		let tasks: Vec<_> = {
			self.peers.all_peers().into_iter()
				.filter_map(|peer_index| {
					let send_headers = self.peers.send_headers(peer_index);

					if send_headers {
						let filtered_blocks_hashes: Vec<_> = new_blocks_hashes.iter()
							.filter(|h| self.peers.filter(peer_index).filter_block(h))
							.collect();
						let chain = self.chain.read();
						let headers: Vec<_> = filtered_blocks_hashes.into_iter()
							.filter_map(|h| chain.block_header_by_hash(&h))
							.collect();
						if !headers.is_empty() {
							Some(Task::SendHeaders(peer_index, headers, ServerTaskIndex::None))
						}
						else {
							None
						}
					} else {
						let inventory: Vec<_> = new_blocks_hashes.iter()
							.filter(|h| self.peers.filter(peer_index).filter_block(h))
							.map(|h| InventoryVector {
								inv_type: InventoryType::MessageBlock,
								hash: h.clone(),
							})
							.collect();
						if !inventory.is_empty() {
							Some(Task::SendInventory(peer_index, inventory, ServerTaskIndex::None))
						} else {
							None
						}
					}
				})
				.collect()
		};

		let mut executor = self.executor.lock();
		for task in tasks {
			executor.execute(task);
		}
	}

	/// Relay new transactions
	fn relay_new_transactions(&mut self, new_transactions: Vec<(H256, &Transaction, u64)>) {
		let tasks: Vec<_> = self.peers.all_peers().into_iter()
			.filter_map(|peer_index| {
				let inventory: Vec<_> = new_transactions.iter()
					.filter(|&&(ref h, tx, tx_fee_rate)| {
						self.peers.filter_mut(peer_index).filter_transaction(h, tx, tx_fee_rate)
					})
					.map(|&(ref h, _, _)| InventoryVector {
						inv_type: InventoryType::MessageTx,
						hash: h.clone(),
					})
					.collect();
				if !inventory.is_empty() {
					Some(Task::SendInventory(peer_index, inventory, ServerTaskIndex::None))
				} else {
					None
				}
			})
			.collect();

		let mut executor = self.executor.lock();
		for task in tasks {
			executor.execute(task);
		}
	}

	/// Process new blocks inventory
	fn process_new_blocks_headers(&mut self, peer_index: usize, mut hashes: Vec<H256>, mut headers: Vec<BlockHeader>) {
		assert_eq!(hashes.len(), headers.len());

		let mut chain = self.chain.write();
		match chain.intersect_with_blocks_headers(&hashes, &headers) {
			HeadersIntersection::NoKnownBlocks(_) if self.state.is_synchronizing() => {
				warn!(target: "sync", "Ignoring {} headers from peer#{}. Unknown and we are synchronizing.", headers.len(), peer_index);
			},
			HeadersIntersection::DbAllBlocksKnown => {
				trace!(target: "sync", "Ignoring {} headers from peer#{}. All blocks are known and in database.", headers.len(), peer_index);
				if self.state.is_synchronizing() {
					// remember peer as useful
					self.peers.useful_peer(peer_index);
				}
			},
			HeadersIntersection::InMemoryNoNewBlocks => {
				trace!(target: "sync", "Ignoring {} headers from peer#{}. All blocks are known and in memory.", headers.len(), peer_index);
				// remember peer as useful
				self.peers.useful_peer(peer_index);
			},
			HeadersIntersection::InMemoryMainNewBlocks(new_block_index)
				| HeadersIntersection::InMemoryForkNewBlocks(new_block_index)
				| HeadersIntersection::DbForkNewBlocks(new_block_index)
				| HeadersIntersection::NoKnownBlocks(new_block_index) => {
				// check that we do not know all blocks in range [new_block_index..]
				// if we know some block => there has been verification error => all headers should be ignored
				// see when_previous_block_verification_failed_fork_is_not_requested for details
				if hashes.iter().skip(new_block_index).any(|h| chain.block_state(h) != BlockState::Unknown) {
					return;
				}

				// schedule new blocks
				let new_blocks_hashes = hashes.split_off(new_block_index);
				let new_blocks_headers = headers.split_off(new_block_index);
				let new_blocks_hashes_len = new_blocks_hashes.len();
				trace!(
					target: "sync", "New {} headers from peer#{}. First {:?}, last: {:?}",
					new_blocks_hashes_len,
					peer_index,
					new_blocks_hashes[0].to_reversed_str(),
					new_blocks_hashes[new_blocks_hashes_len - 1].to_reversed_str()
				);
				chain.schedule_blocks_headers(new_blocks_hashes, new_blocks_headers);
				// remember peer as useful
				self.peers.useful_peer(peer_index);
				// switch to synchronization state
				if !self.state.is_synchronizing() {
					// TODO: NearlySaturated should start when we are in Saturated state && count(new_blocks_headers) is < LIMIT (LIMIT > 1)
					if new_blocks_hashes_len == 1 && !self.state.is_nearly_saturated() {
						self.state = State::NearlySaturated;
					}
					else {
						self.state = State::Synchronizing(time::precise_time_s(), chain.best_storage_block().number);
					}
				}
			}
		}
	}

	/// Process new peer block
	fn process_peer_block(&mut self, peer_index: usize, block_hash: H256, block: Block) -> Option<VecDeque<(H256, Block)>> {
		// prepare list of blocks to verify + make all required changes to the chain
		let mut result: Option<VecDeque<(H256, Block)>> = None;
		let mut chain = self.chain.write();
		match chain.block_state(&block_hash) {
			BlockState::Verifying | BlockState::Stored => {
				// remember peer as useful
				self.peers.useful_peer(peer_index);
			},
			BlockState::Unknown | BlockState::Scheduled | BlockState::Requested => {
				// check parent block state
				match chain.block_state(&block.block_header.previous_header_hash) {
					BlockState::Unknown => {
						if self.state.is_synchronizing() {
							// when synchronizing, we tend to receive all blocks in-order
							trace!(
								target: "sync",
								"Ignoring block {} from peer#{}, because its parent is unknown and we are synchronizing",
								block_hash.to_reversed_str(),
								peer_index
							);
							// remove block from current queue
							chain.forget_block(&block_hash);
							// remove orphaned blocks
							let removed_blocks_hashes: Vec<_> = self.orphaned_blocks_pool.remove_blocks_for_parent(&block_hash).into_iter().map(|t| t.0).collect();
							chain.forget_blocks_leave_header(&removed_blocks_hashes);
						} else {
							// remove this block from the queue
							chain.forget_block_leave_header(&block_hash);
							// remember this block as unknown
							if !self.orphaned_blocks_pool.contains_unknown_block(&block_hash) {
								self.orphaned_blocks_pool.insert_unknown_block(block_hash, block);
							}
						}
					},
					BlockState::Verifying | BlockState::Stored => {
						// remember peer as useful
						self.peers.useful_peer(peer_index);
						// schedule verification
						let mut blocks_to_verify: VecDeque<(H256, Block)> = VecDeque::new();
						blocks_to_verify.push_back((block_hash.clone(), block));
						blocks_to_verify.extend(self.orphaned_blocks_pool.remove_blocks_for_parent(&block_hash));
						// forget blocks we are going to process
						let blocks_hashes_to_forget: Vec<_> = blocks_to_verify.iter().map(|t| t.0.clone()).collect();
						chain.forget_blocks_leave_header(&blocks_hashes_to_forget);
						// remember that we are verifying these blocks
						let blocks_headers_to_verify: Vec<_> = blocks_to_verify.iter().map(|&(ref h, ref b)| (h.clone(), b.block_header.clone())).collect();
						chain.verify_blocks(blocks_headers_to_verify);
						// remember that we are verifying block from this peer
						self.verifying_blocks_by_peer.insert(block_hash.clone(), peer_index);
						match self.verifying_blocks_futures.entry(peer_index) {
							Entry::Occupied(mut entry) => {
								entry.get_mut().0.insert(block_hash.clone());
							},
							Entry::Vacant(entry) => {
								let mut block_hashes = HashSet::new();
								block_hashes.insert(block_hash.clone());
								entry.insert((block_hashes, Vec::new()));
							}
						}
						result = Some(blocks_to_verify);
					},
					BlockState::Requested | BlockState::Scheduled => {
						// remember peer as useful
						self.peers.useful_peer(peer_index);
						// remember as orphan block
						self.orphaned_blocks_pool.insert_orphaned_block(block_hash, block);
					}
				}
			},
		}

		result
	}

	/// Process new peer transaction
	fn process_peer_transaction(&mut self, peer_index: Option<usize>, hash: H256, transaction: Transaction) -> Option<VecDeque<(H256, Transaction)>> {
		// if we are in synchronization state, we will ignore this message
		if self.state.is_synchronizing() {
			return None;
		}

		// mark peer as useful (TODO: remove after self.all_peers() would be all peers, not sync one)
		if let Some(peer_index) = peer_index {
			self.peers.useful_peer(peer_index);
		}

		// else => verify transaction + it's orphans and then add to the memory pool
		let mut chain = self.chain.write();

		// if any parent transaction is unknown => we have orphan transaction => remember in orphan pool
		let unknown_parents: HashSet<H256> = transaction.inputs.iter()
			.filter(|input| chain.transaction_state(&input.previous_output.hash) == TransactionState::Unknown)
			.map(|input| input.previous_output.hash.clone())
			.collect();
		if !unknown_parents.is_empty() {
			self.orphaned_transactions_pool.insert(hash, transaction, unknown_parents);
			return None;
		}

		// else verify && insert this transaction && all dependent orphans
		let mut transactons: VecDeque<(H256, Transaction)> = VecDeque::new();
		transactons.push_back((hash.clone(), transaction));
		transactons.extend(self.orphaned_transactions_pool.remove_transactions_for_parent(&hash));
		// remember that we are verifying these transactions
		for &(ref h, ref tx) in &transactons {
			chain.verify_transaction(h.clone(), tx.clone());
		}
		Some(transactons)
	}

	fn prepare_blocks_requests_tasks(&mut self, peers: Vec<usize>, mut hashes: Vec<H256>) -> Vec<Task> {
		use std::mem::swap;

		let chunk_size = min(MAX_BLOCKS_IN_REQUEST, max(hashes.len() as u32, MIN_BLOCKS_IN_REQUEST));
		let last_peer_index = peers.len() - 1;
		let mut tasks: Vec<Task> = Vec::new();
		for (peer_index, peer) in peers.into_iter().enumerate() {
			// we have to request all blocks => we will request last peer for all remaining blocks
			let peer_chunk_size = if peer_index == last_peer_index { hashes.len() } else { min(hashes.len(), chunk_size as usize) };
			if peer_chunk_size == 0 {
				break;
			}

			let mut chunk_hashes = hashes.split_off(peer_chunk_size);
			swap(&mut chunk_hashes, &mut hashes);

			self.peers.on_blocks_requested(peer, &chunk_hashes);
			tasks.push(Task::RequestBlocks(peer, chunk_hashes));
		}

		assert_eq!(hashes.len(), 0);
		tasks
	}

	/// Switch to saturated state
	fn switch_to_saturated_state(&mut self) {
		if self.state.is_saturated() {
			return;
		}

		self.state = State::Saturated;
		self.peers.reset();

		// remove sync orphans, but leave unknown orphans until they'll be removed by management thread
		let removed_orphans = self.orphaned_blocks_pool.remove_known_blocks();

		// leave currently verifying blocks
		{
			let mut chain = self.chain.write();
			chain.forget_blocks(&removed_orphans);
			chain.forget_all_blocks_with_state(BlockState::Requested);
			chain.forget_all_blocks_with_state(BlockState::Scheduled);

			use time;
			info!(target: "sync", "{:?} @ Switched to saturated state. Chain information: {:?}",
				time::strftime("%H:%M:%S", &time::now()).unwrap(),
				chain.information());
		}

		// finally - ask all known peers for their best blocks inventory, in case if some peer
		// has lead us to the fork
		// + ask all peers for their memory pool
		{
			let mut executor = self.executor.lock();
			for peer in self.peers.all_peers() {
				executor.execute(Task::RequestBlocksHeaders(peer));
				executor.execute(Task::RequestMemoryPool(peer));
			}
		}
	}

	/// Awake threads, waiting for this block
	fn awake_waiting_threads(&mut self, hash: &H256) {
		// find a peer, which has supplied us with this block
		if let Entry::Occupied(block_entry) = self.verifying_blocks_by_peer.entry(hash.clone()) {
			let peer_index = *block_entry.get();
			// find a # of blocks, which this thread has supplied
			if let Entry::Occupied(mut entry) = self.verifying_blocks_futures.entry(peer_index) {
				let is_last_block = {
					let &mut (ref mut waiting, ref mut futures) = entry.get_mut();
					waiting.remove(hash);
					// if this is the last block => awake waiting threads
					let is_last_block = waiting.is_empty();
					if is_last_block {
						for future in futures.drain(..) {
							future.wait().expect("no-error future");
						}
					}
					is_last_block
				};

				if is_last_block {
					entry.remove_entry();
				}
			}
			block_entry.remove_entry();
		}
	}

	/// Print synchronization information
	fn print_synchronization_information(&mut self) {
		if let State::Synchronizing(timestamp, num_of_blocks) = self.state {
			let chain = self.chain.read();

			let new_timestamp = time::precise_time_s();
			let timestamp_diff = new_timestamp - timestamp;
			let new_num_of_blocks = chain.best_storage_block().number;
			let blocks_diff = if new_num_of_blocks > num_of_blocks { new_num_of_blocks - num_of_blocks } else { 0 };
			if timestamp_diff >= 60.0 || blocks_diff > 1000 {
				self.state = State::Synchronizing(time::precise_time_s(), new_num_of_blocks);

				use time;
				info!(target: "sync", "{:?} @ Processed {} blocks in {} seconds. Chain information: {:?}"
					, time::strftime("%H:%M:%S", &time::now()).unwrap()
					, blocks_diff, timestamp_diff
					, chain.information());
			}
		}
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use parking_lot::{Mutex, RwLock};
	use tokio_core::reactor::{Core, Handle};
	use chain::{Block, Transaction};
	use message::common::{InventoryVector, InventoryType};
	use message::types;
	use super::{Client, Config, SynchronizationClient, SynchronizationClientCore};
	use connection_filter::tests::*;
	use synchronization_executor::Task;
	use synchronization_chain::{Chain, ChainRef};
	use synchronization_executor::tests::DummyTaskExecutor;
	use synchronization_verifier::tests::DummyVerifier;
	use synchronization_server::ServerTaskIndex;
	use primitives::hash::H256;
	use p2p::event_loop;
	use test_data;
	use db;
	use devtools::RandomTempPath;

	fn create_disk_storage() -> db::SharedStore {
		let path = RandomTempPath::create_dir();
		Arc::new(db::Storage::new(path.as_path()).unwrap())
	}

	fn create_sync(storage: Option<db::SharedStore>, verifier: Option<DummyVerifier>) -> (Core, Handle, Arc<Mutex<DummyTaskExecutor>>, ChainRef, Arc<Mutex<SynchronizationClient<DummyTaskExecutor, DummyVerifier>>>) {
		let event_loop = event_loop();
		let handle = event_loop.handle();
		let storage = match storage {
			Some(storage) => storage,
			None => Arc::new(db::TestStorage::with_genesis_block()),
		};
		let chain = ChainRef::new(RwLock::new(Chain::new(storage.clone())));
		let executor = DummyTaskExecutor::new();
		let config = Config { threads_num: 1 };

		let client_core = SynchronizationClientCore::new(config, &handle, executor.clone(), chain.clone());
		let mut verifier = verifier.unwrap_or_default();
		verifier.set_sink(client_core.clone());
		let client = SynchronizationClient::new(client_core, verifier);
		(event_loop, handle, executor, chain, client)
	}

	#[test]
	fn synchronization_saturated_on_start() {
		let (_, _, _, _, sync) = create_sync(None, None);
		let sync = sync.lock();
		let info = sync.information();
		assert!(!info.state.is_synchronizing());
		assert_eq!(info.orphaned_blocks, 0);
		assert_eq!(info.orphaned_transactions, 0);
	}

	#[test]
	fn synchronization_in_order_block_path_nearly_saturated() {
		let (_, _, executor, _, sync) = create_sync(None, None);

		let mut sync = sync.lock();
		let block1: Block = test_data::block_h1();
		let block2: Block = test_data::block_h2();

		sync.on_new_blocks_headers(5, vec![block1.block_header.clone()]);
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(5), Task::RequestBlocks(5, vec![block1.hash()])]);
		assert!(sync.information().state.is_nearly_saturated());
		assert_eq!(sync.information().orphaned_blocks, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push unknown block => will be queued as orphan
		sync.on_peer_block(5, block2);
		assert!(sync.information().state.is_nearly_saturated());
		assert_eq!(sync.information().orphaned_blocks, 1);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push requested block => should be moved to the test storage && orphan should be moved
		sync.on_peer_block(5, block1);
		assert!(sync.information().state.is_saturated());
		assert_eq!(sync.information().orphaned_blocks, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 3);
		// we have just requested new `inventory` from the peer => peer is forgotten
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 0);
	}

	#[test]
	fn synchronization_out_of_order_block_path() {
		let (_, _, _, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_new_blocks_headers(5, vec![test_data::block_h1().block_header.clone(), test_data::block_h2().block_header.clone()]);
		sync.on_peer_block(5, test_data::block_h169());

		// out-of-order block was presented by the peer
		assert!(sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned_blocks, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 2);
		assert_eq!(sync.information().chain.stored, 1);
		// we have just requested new `inventory` from the peer => peer is forgotten
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);
		// TODO: check that peer is penalized
	}

	#[test]
	fn synchronization_parallel_peers() {
		let (_, _, executor, _, sync) = create_sync(None, None);

		let block1: Block = test_data::block_h1();
		let block2: Block = test_data::block_h2();

		{
			let mut sync = sync.lock();
			// not synchronizing after start
			assert!(sync.information().state.is_saturated());
			// receive inventory from new peer#1
			sync.on_new_blocks_headers(1, vec![block1.block_header.clone()]);
			assert_eq!(sync.information().chain.requested, 1);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert!(sync.information().state.is_nearly_saturated());
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestBlocks(1, vec![block1.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive inventory from new peer#2
			sync.on_new_blocks_headers(2, vec![block1.block_header.clone(), block2.block_header.clone()]);
			assert_eq!(sync.information().chain.requested, 2);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert!(sync.information().state.is_synchronizing());
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(2), Task::RequestBlocks(2, vec![block2.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive block from peer#2
			sync.on_peer_block(2, block2);
			assert!(sync.information().chain.requested == 2
				&& sync.information().orphaned_blocks == 1);
			// receive block from peer#1
			sync.on_peer_block(1, block1);

			assert!(sync.information().chain.requested == 0
				&& sync.information().orphaned_blocks == 0
				&& sync.information().chain.stored == 3);
		}
	}

	#[test]
	fn synchronization_reset_when_peer_is_disconnected() {
		let (_, _, _, _, sync) = create_sync(None, None);

		// request new blocks
		{
			let mut sync = sync.lock();
			sync.on_new_blocks_headers(1, vec![test_data::block_h1().block_header]);
			assert!(sync.information().state.is_nearly_saturated());
		}

		// lost connection to peer => synchronization state lost
		{
			let mut sync = sync.lock();
			sync.on_peer_disconnected(1);
			assert!(sync.information().state.is_saturated());
		}
	}

	#[test]
	fn synchronization_not_starting_when_receiving_known_blocks() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();
		// saturated => receive inventory with known blocks only
		sync.on_new_blocks_headers(1, vec![test_data::genesis().block_header]);
		// => no need to start synchronization
		assert!(!sync.information().state.is_nearly_saturated());
		// => no synchronization tasks are scheduled
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn synchronization_asks_for_inventory_after_saturating() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();
		let block = test_data::block_h1();
		sync.on_new_blocks_headers(1, vec![block.block_header.clone()]);
		sync.on_new_blocks_headers(2, vec![block.block_header.clone()]);
		executor.lock().take_tasks();
		sync.on_peer_block(2, block.clone());

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 5);
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocksHeaders(1)));
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocksHeaders(2)));
		assert!(tasks.iter().any(|t| t == &Task::RequestMemoryPool(1)));
		assert!(tasks.iter().any(|t| t == &Task::RequestMemoryPool(2)));

		let inventory = vec![InventoryVector { inv_type: InventoryType::MessageBlock, hash: block.hash() }];
		assert!(tasks.iter().any(|t| t == &Task::SendInventory(1, inventory.clone(), ServerTaskIndex::None)));
	}

	#[test]
	fn synchronization_remembers_correct_block_headers_in_order() {
		let (_, _, executor, chain, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 2);
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocksHeaders(1)));
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocks(1, vec![b1.hash(), b2.hash()])));

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 2);
			assert_eq!(chain.information().headers.total, 2);
		}

		sync.on_peer_block(1, b1);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 1);
			assert_eq!(chain.information().headers.total, 1);
		}

		sync.on_peer_block(1, b2);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestMemoryPool(1)]);

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 0);
			assert_eq!(chain.information().headers.total, 0);
		}
	}

	#[test]
	fn synchronization_remembers_correct_block_headers_out_of_order() {
		let (_, _, executor, chain, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 2);
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocksHeaders(1)));
		assert!(tasks.iter().any(|t| t == &Task::RequestBlocks(1, vec![b1.hash(), b2.hash()])));

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 2);
			assert_eq!(chain.information().headers.total, 2);
		}

		sync.on_peer_block(1, b2);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 2);
			assert_eq!(chain.information().headers.total, 2);
		}

		sync.on_peer_block(1, b1);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestMemoryPool(1)]);

		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 0);
			assert_eq!(chain.information().headers.total, 0);
		}
	}

	#[test]
	fn synchronization_ignores_unknown_block_headers() {
		let (_, _, executor, chain, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b169 = test_data::block_h169();
		sync.on_new_blocks_headers(1, vec![b169.block_header]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);

		let chain = chain.read();
		assert_eq!(chain.information().headers.best, 0);
		assert_eq!(chain.information().headers.total, 0);
	}

	#[test]
	fn synchronization_works_for_forks_from_db_best_block() {
		let storage = create_disk_storage();
		let genesis = test_data::genesis();
		storage.insert_block(&genesis).expect("no db error");

		let (_, _, executor, chain, sync) = create_sync(Some(storage), None);
		let genesis_header = &genesis.block_header;
		let fork1 = test_data::build_n_empty_blocks_from(2, 100, &genesis_header);
		let fork2 = test_data::build_n_empty_blocks_from(3, 200, &genesis_header);

		let mut sync = sync.lock();
		sync.on_new_blocks_headers(1, vec![fork1[0].block_header.clone(), fork1[1].block_header.clone()]);
		sync.on_new_blocks_headers(2, vec![fork2[0].block_header.clone(), fork2[1].block_header.clone(), fork2[2].block_header.clone()]);

		let tasks = { executor.lock().take_tasks() };
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1),
			Task::RequestBlocks(1, vec![fork1[0].hash(), fork1[1].hash()]),
			Task::RequestBlocksHeaders(2),
			Task::RequestBlocks(2, vec![fork2[0].hash(), fork2[1].hash(), fork2[2].hash()]),
		]);

		sync.on_peer_block(2, fork2[0].clone());
		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork2[0].hash());
			assert_eq!(chain.best_storage_block().number, 1);
		}

		sync.on_peer_block(1, fork1[0].clone());
		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork2[0].hash());
			assert_eq!(chain.best_storage_block().number, 1);
		}

		sync.on_peer_block(1, fork1[1].clone());
		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork1[1].hash());
			assert_eq!(chain.best_storage_block().number, 2);
		}

		sync.on_peer_block(2, fork2[1].clone());
		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork1[1].hash());
			assert_eq!(chain.best_storage_block().number, 2);
		}

		sync.on_peer_block(2, fork2[2].clone());
		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork2[2].hash());
			assert_eq!(chain.best_storage_block().number, 3);
		}
	}

	#[test]
	fn synchronization_works_for_forks_long_after_short() {
		let storage = create_disk_storage();
		let genesis = test_data::genesis();
		storage.insert_block(&genesis).expect("no db error");

		let (_, _, executor, chain, sync) = create_sync(Some(storage), None);
		let common_block = test_data::block_builder().header().parent(genesis.hash()).build().build();
		let fork1 = test_data::build_n_empty_blocks_from(2, 100, &common_block.block_header);
		let fork2 = test_data::build_n_empty_blocks_from(3, 200, &common_block.block_header);

		let mut sync = sync.lock();
		sync.on_new_blocks_headers(1, vec![common_block.block_header.clone(), fork1[0].block_header.clone(), fork1[1].block_header.clone()]);
		sync.on_new_blocks_headers(2, vec![common_block.block_header.clone(), fork2[0].block_header.clone(), fork2[1].block_header.clone(), fork2[2].block_header.clone()]);

		let tasks = { executor.lock().take_tasks() };
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1),
			Task::RequestBlocks(1, vec![common_block.hash(), fork1[0].hash(), fork1[1].hash()]),
			Task::RequestBlocksHeaders(2),
			Task::RequestBlocks(2, vec![fork2[0].hash(), fork2[1].hash(), fork2[2].hash()]),
		]);

		// TODO: this will change from 3 to 4 after longest fork will be stored in the BestHeadersChain
		// however id doesn't affect sync process, as it is shown below
		{
			let chain = chain.read();
			assert_eq!(chain.information().headers.best, 3);
			assert_eq!(chain.information().headers.total, 3);
		}

		sync.on_peer_block(1, common_block.clone());
		sync.on_peer_block(1, fork1[0].clone());
		sync.on_peer_block(1, fork1[1].clone());
		sync.on_peer_block(2, fork2[0].clone());
		sync.on_peer_block(2, fork2[1].clone());
		sync.on_peer_block(2, fork2[2].clone());

		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().hash, fork2[2].hash());
			assert_eq!(chain.best_storage_block().number, 4);
		}
	}

	#[test]
	fn accept_out_of_order_blocks_when_saturated() {
		let (_, _, _, chain, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_peer_block(1, test_data::block_h2());
		assert_eq!(sync.information().orphaned_blocks, 1);

		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().number, 0);
		}

		sync.on_peer_block(1, test_data::block_h1());
		assert_eq!(sync.information().orphaned_blocks, 0);

		{
			let chain = chain.read();
			assert_eq!(chain.best_storage_block().number, 2);
		}
	}

	#[test]
	fn do_not_rerequest_unknown_block_in_inventory() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_peer_block(1, test_data::block_h2());
		sync.on_new_blocks_inventory(1, vec![test_data::block_h1().hash(), test_data::block_h2().hash()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocks(1, vec![test_data::block_h1().hash()])]);
	}

	#[test]
	fn blocks_rerequested_on_peer_disconnect() {
		let (_, _, executor, _, sync) = create_sync(None, None);

		let block1: Block = test_data::block_h1();
		let block2: Block = test_data::block_h2();

		{
			let mut sync = sync.lock();
			// receive inventory from new peer#1
			sync.on_new_blocks_headers(1, vec![block1.block_header.clone()]);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestBlocks(1, vec![block1.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive inventory from new peer#2
			sync.on_new_blocks_headers(2, vec![block1.block_header.clone(), block2.block_header.clone()]);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(2), Task::RequestBlocks(2, vec![block2.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// peer#1 is disconnected && it has pending blocks requests => ask peer#2
			sync.on_peer_disconnected(1);
			// blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestBlocks(2, vec![block1.hash()])]);
		}
	}

	#[test]
	fn sync_after_db_insert_nonfatal_fail() {
		// TODO: implement me
	}

	#[test]
	fn peer_removed_from_sync_after_responding_with_requested_block_notfound() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestBlocks(1, vec![b1.hash(), b2.hash()])]);

		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.unuseful, 0);
		assert_eq!(sync.information().peers.active, 1);

		sync.on_peer_blocks_notfound(1, vec![b1.hash()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestMemoryPool(1)]);

		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.unuseful, 1);
		assert_eq!(sync.information().peers.active, 0);
	}

	#[test]
	fn peer_not_removed_from_sync_after_responding_with_requested_block_notfound() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1), Task::RequestBlocks(1, vec![b1.hash(), b2.hash()])]);

		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.unuseful, 0);
		assert_eq!(sync.information().peers.active, 1);

		sync.on_peer_blocks_notfound(1, vec![test_data::block_h170().hash()]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);

		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.unuseful, 0);
		assert_eq!(sync.information().peers.active, 1);
	}

	#[test]
	fn transaction_is_not_requested_when_synchronizing() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		assert!(sync.information().state.is_synchronizing());
		{ executor.lock().take_tasks(); } // forget tasks

		sync.on_new_transactions_inventory(0, vec![H256::from(0)]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn transaction_is_requested_when_not_synchronizing() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_new_transactions_inventory(0, vec![H256::from(0)]);

		{
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestTransactions(0, vec![H256::from(0)])]);
		}

		let b1 = test_data::block_h1();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone()]);

		assert!(sync.information().state.is_nearly_saturated());
		{ executor.lock().take_tasks(); } // forget tasks

		sync.on_new_transactions_inventory(0, vec![H256::from(1)]);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestTransactions(0, vec![H256::from(1)])]);
	}

	#[test]
	fn same_transaction_can_be_requested_twice() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_new_transactions_inventory(0, vec![H256::from(0)]);

		{
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestTransactions(0, vec![H256::from(0)])]);
		}

		sync.on_new_transactions_inventory(0, vec![H256::from(0)]);

		{
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestTransactions(0, vec![H256::from(0)])]);
		}
	}

	#[test]
	fn known_transaction_is_not_requested() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_new_transactions_inventory(0, vec![test_data::genesis().transactions[0].hash(), H256::from(0)]);
		assert_eq!(executor.lock().take_tasks(), vec![Task::RequestTransactions(0, vec![H256::from(0)])]);
	}

	#[test]
	fn transaction_is_not_accepted_when_synchronizing() {
		let (_, _, _, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		let b1 = test_data::block_h1();
		let b2 = test_data::block_h2();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone(), b2.block_header.clone()]);

		assert!(sync.information().state.is_synchronizing());

		sync.on_peer_transaction(1, Transaction::default());

		assert_eq!(sync.information().chain.transactions.transactions_count, 0);
	}

	#[test]
	fn transaction_is_accepted_when_not_synchronizing() {
		let (_, _, _, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_peer_transaction(1, test_data::TransactionBuilder::with_version(1).into());
		assert_eq!(sync.information().chain.transactions.transactions_count, 1);

		let b1 = test_data::block_h1();
		sync.on_new_blocks_headers(1, vec![b1.block_header.clone()]);

		assert!(sync.information().state.is_nearly_saturated());

		sync.on_peer_transaction(1, test_data::TransactionBuilder::with_version(2).into());
		assert_eq!(sync.information().chain.transactions.transactions_count, 2);
	}

	#[test]
	fn transaction_is_orphaned_when_input_is_unknown() {
		let (_, _, _, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_peer_transaction(1, test_data::TransactionBuilder::with_default_input(0).into());
		assert_eq!(sync.information().chain.transactions.transactions_count, 0);
		assert_eq!(sync.information().orphaned_transactions, 1);
	}

	#[test]
	fn orphaned_transaction_is_verified_when_input_is_received() {
		let chain = &mut test_data::ChainBuilder::new();
		test_data::TransactionBuilder::with_output(10).store(chain)		// t0
			.set_input(&chain.at(0), 0).set_output(20).store(chain);	// t0 -> t1

		let (_, _, _, _, sync) = create_sync(None, None);
		let mut sync = sync.lock();

		sync.on_peer_transaction(1, chain.at(1));
		assert_eq!(sync.information().chain.transactions.transactions_count, 0);
		assert_eq!(sync.information().orphaned_transactions, 1);

		sync.on_peer_transaction(1, chain.at(0));
		assert_eq!(sync.information().chain.transactions.transactions_count, 2);
		assert_eq!(sync.information().orphaned_transactions, 0);
	}

	#[test]
	// https://github.com/ethcore/parity-bitcoin/issues/121
	fn when_previous_block_verification_failed_fork_is_not_requested() {
		// got headers [b10, b11, b12] - some fork
		// got headers [b10, b21, b22] - main branch
		// got b10, b11, b12, b21. b22 is requested
		//
		// verifying: [b10, b11, b12, b21]
		// headers_chain: [b10, b11, b12]
		//
		// b21 verification failed => b22 is not removed (since it is not in headers_chain)
		// got new headers [b10, b21, b22, b23] => intersection point is b10 => scheduling [b21, b22, b23]
		//
		// block queue is empty => new tasks => requesting [b21, b22] => panic in hash_queue
		//
		// => do not trust first intersection point - check each hash when scheduling hashes.
		// If at least one hash is known => previous verification failed => drop all headers.

		let genesis = test_data::genesis();
		let b10 = test_data::block_builder().header().parent(genesis.hash()).build().build();

		let b11 = test_data::block_builder().header().nonce(1).parent(b10.hash()).build().build();
		let b12 = test_data::block_builder().header().parent(b11.hash()).build().build();

		let b21 = test_data::block_builder().header().nonce(2).parent(b10.hash()).build().build();
		let b22 = test_data::block_builder().header().parent(b21.hash()).build().build();
		let b23 = test_data::block_builder().header().parent(b22.hash()).build().build();

		// simulate verification during b21 verification
		let mut dummy_verifier = DummyVerifier::default();
		dummy_verifier.error_when_verifying(b21.hash(), "simulated");

		let (_, _, _, _, sync) = create_sync(None, Some(dummy_verifier));

		let mut sync = sync.lock();

		sync.on_new_blocks_headers(1, vec![b10.block_header.clone(), b11.block_header.clone(), b12.block_header.clone()]);
		sync.on_new_blocks_headers(2, vec![b10.block_header.clone(), b21.block_header.clone(), b22.block_header.clone()]);

		sync.on_peer_block(1, b10.clone());
		sync.on_peer_block(1, b11);
		sync.on_peer_block(1, b12);

		sync.on_peer_block(2, b21.clone());

		// should not panic here
		sync.on_new_blocks_headers(2, vec![b10.block_header.clone(), b21.block_header.clone(),
			b22.block_header.clone(), b23.block_header.clone()]);
	}

	#[test]
	fn relay_new_block_when_in_saturated_state() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let genesis = test_data::genesis();
		let b0 = test_data::block_builder().header().parent(genesis.hash()).build().build();
		let b1 = test_data::block_builder().header().parent(b0.hash()).build().build();
		let b2 = test_data::block_builder().header().parent(b1.hash()).build().build();
		let b3 = test_data::block_builder().header().parent(b2.hash()).build().build();

		let mut sync = sync.lock();
		sync.on_new_blocks_headers(1, vec![b0.block_header.clone(), b1.block_header.clone()]);
		sync.on_peer_block(1, b0.clone());
		sync.on_peer_block(1, b1.clone());

		// we were in synchronization state => block is not relayed
		{
			let tasks = executor.lock().take_tasks();
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1),
				Task::RequestBlocks(1, vec![b0.hash(), b1.hash()]),
				Task::RequestBlocksHeaders(1),
				Task::RequestMemoryPool(1)
			]);
		}

		sync.on_peer_block(2, b2.clone());

		// we were in saturated state => block is relayed
		{
			let tasks = executor.lock().take_tasks();
			let inventory = vec![InventoryVector { inv_type: InventoryType::MessageBlock, hash: b2.hash() }];
			assert_eq!(tasks, vec![Task::RequestBlocksHeaders(2), Task::SendInventory(1, inventory, ServerTaskIndex::None)]);
		}

		sync.on_new_blocks_headers(1, vec![b3.block_header.clone()]);
		sync.on_peer_block(1, b3.clone());

		// we were in nearly saturated state => block is relayed
		{
			let tasks = executor.lock().take_tasks();
			let inventory = vec![InventoryVector { inv_type: InventoryType::MessageBlock, hash: b3.hash() }];
			assert!(tasks.iter().any(|t| t == &Task::SendInventory(2, inventory.clone(), ServerTaskIndex::None)));
		}
	}

	#[test]
	fn relay_new_transaction_when_in_saturated_state() {
		let (_, _, executor, _, sync) = create_sync(None, None);

		let tx1: Transaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_output(20).into();
		let tx2_hash = tx2.hash();

		let mut sync = sync.lock();
		sync.on_peer_transaction(1, tx1);
		sync.on_peer_transaction(2, tx2);

		let tasks = { executor.lock().take_tasks() };
		let inventory = vec![InventoryVector { inv_type: InventoryType::MessageTx, hash: tx2_hash }];
		assert_eq!(tasks, vec![Task::SendInventory(1, inventory, ServerTaskIndex::None)]);
	}

	#[test]
	fn relay_new_transaction_with_bloom_filter() {
		let (_, _, executor, _, sync) = create_sync(None, None);

		let tx1: Transaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_output(20).into();
		let tx3: Transaction = test_data::TransactionBuilder::with_output(30).into();
		let tx1_hash = tx1.hash();
		let tx2_hash = tx2.hash();
		let tx3_hash = tx3.hash();

		let mut sync = sync.lock();
		// peer#1 wants tx1
		sync.on_peer_connected(1);
		sync.on_peer_filterload(1, &default_filterload());
		sync.on_peer_filteradd(1, &make_filteradd(&*tx1_hash));
		// peer#2 wants tx2
		sync.on_peer_connected(2);
		sync.on_peer_filterload(2, &default_filterload());
		sync.on_peer_filteradd(2, &make_filteradd(&*tx2_hash));
		// peer#3 wants tx1 + tx2 transactions
		sync.on_peer_connected(3);
		sync.on_peer_filterload(3, &default_filterload());
		sync.on_peer_filteradd(3, &make_filteradd(&*tx1_hash));
		sync.on_peer_filteradd(3, &make_filteradd(&*tx2_hash));
		// peer#4 has default behaviour (no filter)
		sync.on_peer_connected(4);
		// peer#5 wants some other transactions
		sync.on_peer_connected(5);
		sync.on_peer_filterload(5, &default_filterload());
		sync.on_peer_filteradd(5, &make_filteradd(&*tx3_hash));

		// tx1 is relayed to peers: 1, 3, 4
		sync.on_peer_transaction(6, tx1);

		let tasks = { executor.lock().take_tasks() };
		let inventory = vec![InventoryVector { inv_type: InventoryType::MessageTx, hash: tx1_hash }];
		assert_eq!(tasks, vec![
			Task::SendInventory(1, inventory.clone(), ServerTaskIndex::None),
			Task::SendInventory(3, inventory.clone(), ServerTaskIndex::None),
			Task::SendInventory(4, inventory.clone(), ServerTaskIndex::None),
		]);

		// tx2 is relayed to peers: 2, 3, 4
		sync.on_peer_transaction(6, tx2);

		let tasks = { executor.lock().take_tasks() };
		let inventory = vec![InventoryVector { inv_type: InventoryType::MessageTx, hash: tx2_hash }];
		assert_eq!(tasks, vec![
			Task::SendInventory(2, inventory.clone(), ServerTaskIndex::None),
			Task::SendInventory(3, inventory.clone(), ServerTaskIndex::None),
			Task::SendInventory(4, inventory.clone(), ServerTaskIndex::None),
		]);
	}

	#[test]
	fn relay_new_block_after_sendheaders() {
		let (_, _, executor, _, sync) = create_sync(None, None);
		let genesis = test_data::genesis();
		let b0 = test_data::block_builder().header().parent(genesis.hash()).build().build();

		let mut sync = sync.lock();
		sync.on_peer_connected(1);
		sync.on_peer_connected(2);
		sync.on_peer_sendheaders(2);
		sync.on_peer_connected(3);

		// igonore tasks
		{ executor.lock().take_tasks(); }

		sync.on_peer_block(1, b0.clone());

		let tasks = executor.lock().take_tasks();
		let inventory = vec![InventoryVector { inv_type: InventoryType::MessageBlock, hash: b0.hash() }];
		let headers = vec![b0.block_header.clone()];
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(1),
			Task::SendHeaders(2, headers, ServerTaskIndex::None),
			Task::SendInventory(3, inventory, ServerTaskIndex::None),
		]);
	}

	#[test]
	fn relay_new_transaction_with_feefilter() {
		let (_, _, executor, chain, sync) = create_sync(None, None);

		let b1 = test_data::block_builder().header().parent(test_data::genesis().hash()).build()
			.transaction().output().value(1_000_000).build().build()
			.build(); // genesis -> b1
		let tx0 = b1.transactions[0].clone();
		let tx1: Transaction = test_data::TransactionBuilder::with_output(800_000).add_input(&tx0, 0).into();
		let tx1_hash = tx1.hash();

		let mut sync = sync.lock();
		sync.on_peer_connected(1);
		sync.on_peer_connected(2);
		sync.on_peer_connected(3);
		sync.on_peer_connected(4);

		sync.on_peer_block(1, b1);

		{
			use miner::transaction_fee_rate;
			let chain = chain.read();
			assert_eq!(transaction_fee_rate(&*chain, &tx1), 3333); // 200_000 / 60
		}

		sync.on_peer_feefilter(2, &types::FeeFilter { fee_rate: 3000, });
		sync.on_peer_feefilter(3, &types::FeeFilter { fee_rate: 4000, });

		// forget previous tasks
		{ executor.lock().take_tasks(); }

		sync.on_peer_transaction(1, tx1);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![
			Task::SendInventory(2, vec![
				InventoryVector {
					inv_type: InventoryType::MessageTx,
					hash: tx1_hash.clone(),
				}
			], ServerTaskIndex::None),
			Task::SendInventory(4, vec![
				InventoryVector {
					inv_type: InventoryType::MessageTx,
					hash: tx1_hash.clone(),
				}
			], ServerTaskIndex::None),
		]);
	}

	#[test]
	fn receive_same_unknown_block_twice() {
		let (_, _, _, _, sync) = create_sync(None, None);

		let mut sync = sync.lock();

		sync.on_peer_block(1, test_data::block_h2());
		// should not panic here
		sync.on_peer_block(2, test_data::block_h2());
	}
}
