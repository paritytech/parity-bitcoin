use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;
use db;
use parking_lot::RwLock;
use chain::RepresentH256;
use p2p::OutboundSyncConnectionRef;
use message::common::InventoryType;
use message::types;
use synchronization::{Synchronization, SynchronizationRef, Config as SynchronizationConfig, Task as SynchronizationTask, TaskExecutor as SynchronizationTaskExecutor};
use synchronization_chain::{Chain, ChainRef, BlockState};
use synchronization_executor::LocalSynchronizationTaskExecutor;
use best_block::BestBlock;

/// Thread-safe reference to the `LocalNode`.
/// Locks order:
/// 1) sync Mutex
/// 2) executor Mutex
/// 2) chain RwLock
pub type LocalNodeRef = Arc<LocalNode>;

/// Local synchronization node
pub struct LocalNode {
	/// Throughout counter of synchronization peers
	peer_counter: AtomicUsize,
	/// Synchronization chain
	chain: ChainRef,
	/// Synchronization executor
	executor: Arc<Mutex<LocalSynchronizationTaskExecutor>>,
	/// Synchronization process
	sync: SynchronizationRef<LocalSynchronizationTaskExecutor>,
}

impl LocalNode {
	/// New synchronization node with given storage
	pub fn new(storage: Arc<db::Store>) -> LocalNodeRef {
		let chain = ChainRef::new(RwLock::new(Chain::new(storage.clone())));
		let executor = LocalSynchronizationTaskExecutor::new(chain.clone());
		let sync = Synchronization::new(SynchronizationConfig::default(), executor.clone(), chain.clone());
		Arc::new(LocalNode {
			peer_counter: AtomicUsize::new(0),
			chain: chain,
			executor: executor,
			sync: sync,
		})
	}

	/// Best block hash (including non-verified, requested && non-requested blocks)
	pub fn best_block(&self) -> BestBlock {
		self.chain.read().best_block()
	}

	pub fn create_sync_session(&self, _best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> usize {
		// save connection for future
		let peer_index = self.peer_counter.fetch_add(1, Ordering::SeqCst) + 1;
		trace!(target: "sync", "Creating new sync session with peer#{}", peer_index);

		self.executor.lock().add_peer_connection(peer_index, outbound_connection);
		peer_index
	}

	pub fn start_sync_session(&self, peer_index: usize, _version: u32) {
		trace!(target: "sync", "Starting new sync session with peer#{}", peer_index);

		// request inventory from peer
		self.executor.lock().execute(SynchronizationTask::RequestInventory(peer_index));
	}

	pub fn on_peer_inventory(&self, peer_index: usize, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());

		// TODO: after each `getblocks` message bitcoind responds with two `inventory` messages:
		// (1) with single entry
		// (2) with 500 entries
		// what is (1)?

		// process unknown blocks
		let unknown_blocks: Vec<_> = {
			let chain = self.chain.read();
			message.inventory.iter()
				.filter(|item| item.inv_type == InventoryType::MessageBlock)
				.filter(|item| chain.block_state(&item.hash) == BlockState::Unknown)
				.map(|item| item.hash.clone())
				.collect()
		};

		// if there are unknown blocks => start synchronizing with peer
		if !unknown_blocks.is_empty() {
			self.sync.lock().on_unknown_blocks(peer_index, unknown_blocks);
		}

		// TODO: process unknown transactions, etc...
	}

	pub fn on_peer_getdata(&self, peer_index: usize, _message: types::GetData) {
		trace!(target: "sync", "Got `getdata` message from peer#{}", peer_index);
	}

	pub fn on_peer_getblocks(&self, peer_index: usize, _message: types::GetBlocks) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);
	}

	pub fn on_peer_getheaders(&self, peer_index: usize, _message: types::GetHeaders) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_transaction(&self, _peer_index: usize, _message: types::Tx) {
	}

	pub fn on_peer_block(&self, peer_index: usize, message: types::Block) {
		trace!(target: "sync", "Got `block` message from peer#{}. Block hash: {}", peer_index, message.block.hash());

		// try to process new block
		self.sync.lock().on_peer_block(peer_index, message.block);
	}

	pub fn on_peer_headers(&self, peer_index: usize, _message: types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}", peer_index);
	}

	pub fn on_peer_mempool(&self, peer_index: usize, _message: types::MemPool) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterload(&self, peer_index: usize, _message: types::FilterLoad) {
		trace!(target: "sync", "Got `filterload` message from peer#{}", peer_index);
	}

	pub fn on_peer_filteradd(&self, peer_index: usize, _message: types::FilterAdd) {
		trace!(target: "sync", "Got `filteradd` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterclear(&self, peer_index: usize, _message: types::FilterClear) {
		trace!(target: "sync", "Got `filterclear` message from peer#{}", peer_index);
	}

	pub fn on_peer_merkleblock(&self, peer_index: usize, _message: types::MerkleBlock) {
		trace!(target: "sync", "Got `merkleblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_sendheaders(&self, peer_index: usize, _message: types::SendHeaders) {
		trace!(target: "sync", "Got `sendheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_feefilter(&self, peer_index: usize, _message: types::FeeFilter) {
		trace!(target: "sync", "Got `feefilter` message from peer#{}", peer_index);
	}

	pub fn on_peer_send_compact(&self, peer_index: usize, _message: types::SendCompact) {
		trace!(target: "sync", "Got `sendcmpct` message from peer#{}", peer_index);
	}

	pub fn on_peer_compact_block(&self, peer_index: usize, _message: types::CompactBlock) {
		trace!(target: "sync", "Got `cmpctblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_get_block_txn(&self, peer_index: usize, _message: types::GetBlockTxn) {
		trace!(target: "sync", "Got `getblocktxn` message from peer#{}", peer_index);
	}

	pub fn on_peer_block_txn(&self, peer_index: usize, _message: types::BlockTxn) {
		trace!(target: "sync", "Got `blocktxn` message from peer#{}", peer_index);
	}
}
