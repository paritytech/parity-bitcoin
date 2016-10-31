use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;
use db;
use chain::RepresentH256;
use p2p::OutboundSyncConnectionRef;
use message::common::InventoryType;
use message::types;
use synchronization::{Synchronization, SynchronizationRef, Config as SynchronizationConfig, Task as SynchronizationTask, TaskExecutor as SynchronizationTaskExecutor};
use synchronization_chain::ChainRef;

/// Thread-safe reference to the `LocalNode`.
/// Locks order:
/// 1) sync Mutex
/// 2) executor Mutex
/// 2) chain RwLock
pub type LocalNodeRef<T> = Arc<LocalNode<T>>;

/// Local synchronization node
pub struct LocalNode<T: SynchronizationTaskExecutor + PeersConnections + Send + 'static> {
	/// Throughout counter of synchronization peers
	peer_counter: AtomicUsize,
	/// Synchronization chain
	chain: ChainRef,
	/// Synchronization executor
	executor: Arc<Mutex<T>>,
	/// Synchronization process
	sync: SynchronizationRef<T>,
}

/// Peers list
pub trait PeersConnections {
	fn add_peer_connection(&mut self, peer_index: usize, outbound_connection: OutboundSyncConnectionRef);
	fn remove_peer_connection(&mut self, peer_index: usize);
}

impl<T> LocalNode<T> where T: SynchronizationTaskExecutor + PeersConnections + Send + 'static {
	/// New synchronization node with given storage
	pub fn new(chain: ChainRef, executor: Arc<Mutex<T>>) -> LocalNodeRef<T> {
		let sync = Synchronization::new(SynchronizationConfig::default(), executor.clone(), chain.clone());
		Arc::new(LocalNode {
			peer_counter: AtomicUsize::new(0),
			chain: chain,
			executor: executor,
			sync: sync,
		})
	}

	/// Best block hash (including non-verified, requested && non-requested blocks)
	pub fn best_block(&self) -> db::BestBlock {
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

	pub fn stop_sync_session(&self, peer_index: usize) {
		trace!(target: "sync", "Stopping sync session with peer#{}", peer_index);

		self.executor.lock().remove_peer_connection(peer_index);
		self.sync.lock().on_peer_disconnected(peer_index);
	}

	pub fn on_peer_inventory(&self, peer_index: usize, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());

		// TODO: after each `getblocks` message bitcoind responds with two `inventory` messages:
		// (1) with single entry
		// (2) with 500 entries
		// what is (1)?

		// process blocks first
		let blocks_inventory: Vec<_> = message.inventory.iter()
			.filter(|item| item.inv_type == InventoryType::MessageBlock)
			.map(|item| item.hash.clone())
			.collect();

		// if there are unknown blocks => start synchronizing with peer
		if !blocks_inventory.is_empty() {
			self.sync.lock().on_new_blocks_inventory(peer_index, blocks_inventory);
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

	pub fn on_peer_transaction(&self, peer_index: usize, message: types::Tx) {
		trace!(target: "sync", "Got `transaction` message from peer#{}. Transaction hash: {}", peer_index, message.transaction.hash());
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

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use parking_lot::{Mutex, RwLock};
	use synchronization::Task;
	use synchronization::tests::DummyTaskExecutor;
	use synchronization_chain::Chain;
	use p2p::{OutboundSyncConnection, OutboundSyncConnectionRef};
	use message::types;
	use db;
	use super::LocalNode;

	struct DummyOutboundSyncConnection;

	impl DummyOutboundSyncConnection {
		pub fn new() -> OutboundSyncConnectionRef {
			Box::new(DummyOutboundSyncConnection {})
		}
	}

	impl OutboundSyncConnection for DummyOutboundSyncConnection {
		fn send_inventory(&self, _message: &types::Inv) {}
		fn send_getdata(&self, _message: &types::GetData) {}
		fn send_getblocks(&self, _message: &types::GetBlocks) {}
		fn send_getheaders(&self, _message: &types::GetHeaders) {}
		fn send_transaction(&self, _message: &types::Tx) {}
		fn send_block(&self, _message: &types::Block) {}
		fn send_headers(&self, _message: &types::Headers) {}
		fn send_mempool(&self, _message: &types::MemPool) {}
		fn send_filterload(&self, _message: &types::FilterLoad) {}
		fn send_filteradd(&self, _message: &types::FilterAdd) {}
		fn send_filterclear(&self, _message: &types::FilterClear) {}
		fn send_merkleblock(&self, _message: &types::MerkleBlock) {}
		fn send_sendheaders(&self, _message: &types::SendHeaders) {}
		fn send_feefilter(&self, _message: &types::FeeFilter) {}
		fn send_send_compact(&self, _message: &types::SendCompact) {}
		fn send_compact_block(&self, _message: &types::CompactBlock) {}
		fn send_get_block_txn(&self, _message: &types::GetBlockTxn) {}
		fn send_block_txn(&self, _message: &types::BlockTxn) {}
	}

	#[test]
	fn local_node_request_inventory_on_sync_start() {
		let chain = Arc::new(RwLock::new(Chain::new(Arc::new(db::TestStorage::with_genesis_block()))));
		let executor = Arc::new(Mutex::new(DummyTaskExecutor::default()));
		let local_node = LocalNode::new(chain, executor.clone());
		let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.start_sync_session(peer_index, 0);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 1);
		assert_eq!(tasks, vec![Task::RequestInventory(peer_index)]);
	}
}