use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::Mutex;
use db;
use chain::RepresentH256;
use p2p::OutboundSyncConnectionRef;
use message::common::InventoryType;
use message::types;
use synchronization_client::{Client, SynchronizationClient};
use synchronization_executor::{Task as SynchronizationTask, TaskExecutor as SynchronizationTaskExecutor, LocalSynchronizationTaskExecutor};
use synchronization_server::{Server, SynchronizationServer};

pub type LocalNodeRef = Arc<LocalNode<LocalSynchronizationTaskExecutor, SynchronizationServer, SynchronizationClient<LocalSynchronizationTaskExecutor>>>;

/// Local synchronization node
pub struct LocalNode<T: SynchronizationTaskExecutor + PeersConnections,
	U: Server,
	V: Client> {
	/// Throughout counter of synchronization peers
	peer_counter: AtomicUsize,
	/// Synchronization executor
	executor: Arc<Mutex<T>>,
	/// Synchronization process
	client: Arc<Mutex<V>>,
	/// Synchronization server
	server: Arc<U>,
}

/// Peers list
pub trait PeersConnections {
	fn add_peer_connection(&mut self, peer_index: usize, outbound_connection: OutboundSyncConnectionRef);
	fn remove_peer_connection(&mut self, peer_index: usize);
}

impl<T, U, V> LocalNode<T, U, V> where T: SynchronizationTaskExecutor + PeersConnections,
	U: Server,
	V: Client {
	/// New synchronization node with given storage
	pub fn new(server: Arc<U>, client: Arc<Mutex<V>>, executor: Arc<Mutex<T>>) -> Self {
		LocalNode {
			peer_counter: AtomicUsize::new(0),
			executor: executor,
			client: client,
			server: server,
		}
	}

	/// Best block hash (including non-verified, requested && non-requested blocks)
	pub fn best_block(&self) -> db::BestBlock {
		self.client.lock().best_block()
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
		// TODO: bitcoind doesn't respond to the `getheaders` request while it is synchronizing
		//     but it answers to the `inventory` request
		self.executor.lock().execute(SynchronizationTask::RequestBlocksHeaders(peer_index));
	}

	pub fn stop_sync_session(&self, peer_index: usize) {
		trace!(target: "sync", "Stopping sync session with peer#{}", peer_index);

		self.executor.lock().remove_peer_connection(peer_index);
		self.client.lock().on_peer_disconnected(peer_index);
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
			self.client.lock().on_new_blocks_inventory(peer_index, blocks_inventory);
		}

		// TODO: process unknown transactions, etc...
	}

	pub fn on_peer_getdata(&self, peer_index: usize, message: types::GetData, id: u32) {
		trace!(target: "sync", "Got `getdata` message from peer#{}", peer_index);

		self.server.serve_getdata(peer_index, message, id);
	}

	pub fn on_peer_getblocks(&self, peer_index: usize, message: types::GetBlocks, id: u32) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);

		self.server.serve_getblocks(peer_index, message, id);
	}

	pub fn on_peer_getheaders(&self, peer_index: usize, message: types::GetHeaders, id: u32) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);

		// simulating bitcoind for passing tests: if we are in nearly-saturated state
		// and peer, which has just provided a new blocks to us, is asking for headers
		// => do not serve getheaders until we have fully process his blocks + wait until headers are served before returning
		let need_wait = {
			let (need_wait, waiter) = { self.client.lock().get_peers_nearly_blocks_waiter(peer_index) };
			if let Some(waiter) = waiter {
				waiter.wait();
			}
			need_wait
		};

		self.server.serve_getheaders(peer_index, message, id);
		if need_wait {
			self.server.wait_peer_requests_completed(peer_index);
		}
	}

	pub fn on_peer_transaction(&self, peer_index: usize, message: types::Tx) {
		trace!(target: "sync", "Got `transaction` message from peer#{}. Transaction hash: {}", peer_index, message.transaction.hash());
	}

	pub fn on_peer_block(&self, peer_index: usize, message: types::Block) {
		trace!(target: "sync", "Got `block` message from peer#{}. Block hash: {}", peer_index, message.block.hash());

		// try to process new block
		self.client.lock().on_peer_block(peer_index, message.block);
	}

	pub fn on_peer_headers(&self, peer_index: usize, message: types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}. # of headers: {}", peer_index, message.headers.len());

		if !message.headers.is_empty() {
			self.client.lock().on_new_blocks_headers(peer_index, message.headers);
		}
	}

	pub fn on_peer_mempool(&self, peer_index: usize, _message: types::MemPool, id: u32) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);

		self.server.serve_mempool(peer_index, id);
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

	pub fn on_peer_notfound(&self, peer_index: usize, _message: types::NotFound) {
		trace!(target: "sync", "Got `notfound` message from peer#{}", peer_index);
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use parking_lot::{Mutex, RwLock};
	use chain::RepresentH256;
	use synchronization_executor::Task;
	use synchronization_executor::tests::DummyTaskExecutor;
	use synchronization_client::{Config, SynchronizationClient};
	use synchronization_chain::Chain;
	use p2p::{event_loop, OutboundSyncConnection, OutboundSyncConnectionRef};
	use message::types;
	use message::common::{InventoryVector, InventoryType};
	use db;
	use super::LocalNode;
	use test_data;
	use synchronization_server::ServerTask;
	use synchronization_server::tests::DummyServer;
	use tokio_core::reactor::{Core, Handle};

	struct DummyOutboundSyncConnection;

	impl DummyOutboundSyncConnection {
		pub fn new() -> OutboundSyncConnectionRef {
			Box::new(DummyOutboundSyncConnection {})
		}
	}

	impl OutboundSyncConnection for DummyOutboundSyncConnection {
		fn send_inventory(&self, _message: &types::Inv, _id: u32, _is_final: bool) {}
		fn send_getdata(&self, _message: &types::GetData) {}
		fn send_getblocks(&self, _message: &types::GetBlocks) {}
		fn send_getheaders(&self, _message: &types::GetHeaders) {}
		fn send_transaction(&self, _message: &types::Tx) {}
		fn send_block(&self, _message: &types::Block, _id: u32, _is_final: bool) {}
		fn send_headers(&self, _message: &types::Headers, _id: u32, _is_final: bool) {}
		fn send_mempool(&self, _message: &types::MemPool, _id: u32, _is_final: bool) {}
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
		fn send_notfound(&self, _message: &types::NotFound, _id: u32, _is_final: bool) {}
	}

	fn create_local_node() -> (Core, Handle, Arc<Mutex<DummyTaskExecutor>>, Arc<DummyServer>, LocalNode<DummyTaskExecutor, DummyServer, SynchronizationClient<DummyTaskExecutor>>) {
		let event_loop = event_loop();
		let handle = event_loop.handle();
		let chain = Arc::new(RwLock::new(Chain::new(Arc::new(db::TestStorage::with_genesis_block()))));
		let executor = DummyTaskExecutor::new();
		let server = Arc::new(DummyServer::new());
		let config = Config { threads_num: 1, skip_verification: true };
		let client = SynchronizationClient::new(config, &handle, executor.clone(), chain);
		let local_node = LocalNode::new(server.clone(), client, executor.clone());
		(event_loop, handle, executor, server, local_node)
	}

	#[test]
	fn local_node_request_inventory_on_sync_start() {
		let (_, _, executor, _, local_node) = create_local_node();
		let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		// start sync session
		local_node.start_sync_session(peer_index, 0);
		// => ask for inventory
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(peer_index)]);
	}

	#[test]
	fn local_node_serves_block() {
		let (_, _, _, server, local_node) = create_local_node();
		let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		// peer requests genesis block
		let genesis_block_hash = test_data::genesis().hash();
		let inventory = vec![
			InventoryVector {
				inv_type: InventoryType::MessageBlock,
				hash: genesis_block_hash.clone(),
			}
		];
		let dummy_id = 0;
		local_node.on_peer_getdata(peer_index, types::GetData {
			inventory: inventory.clone()
		}, dummy_id);
		// => `getdata` is served
		let tasks = server.take_tasks();
		assert_eq!(tasks, vec![(peer_index, ServerTask::ServeGetData(inventory))]);
	}
}
