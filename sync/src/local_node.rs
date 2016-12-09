use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use parking_lot::{Mutex, Condvar};
use db;
use chain::Transaction;
use p2p::OutboundSyncConnectionRef;
use message::common::{InventoryType, InventoryVector};
use message::types;
use synchronization_client::{Client, SynchronizationClient, BlockAnnouncementType};
use synchronization_executor::{Task as SynchronizationTask, TaskExecutor as SynchronizationTaskExecutor, LocalSynchronizationTaskExecutor};
use synchronization_server::{Server, SynchronizationServer};
use synchronization_verifier::{AsyncVerifier, TransactionVerificationSink};
use primitives::hash::H256;

// TODO: check messages before processing (filterload' filter is max 36000, nHashFunc is <= 50, etc)

pub type LocalNodeRef = Arc<LocalNode<LocalSynchronizationTaskExecutor, SynchronizationServer, SynchronizationClient<LocalSynchronizationTaskExecutor, AsyncVerifier>>>;

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

/// Transaction accept verification sink
struct TransactionAcceptSink {
	data: Arc<TransactionAcceptSinkData>,
}

#[derive(Default)]
struct TransactionAcceptSinkData {
	result: Mutex<Option<Result<H256, String>>>,
	waiter: Condvar,
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

		self.client.lock().on_peer_connected(peer_index);
		self.executor.lock().add_peer_connection(peer_index, outbound_connection);
		peer_index
	}

	pub fn start_sync_session(&self, peer_index: usize, _version: u32) {
		trace!(target: "sync", "Starting new sync session with peer#{}", peer_index);

		// request inventory from peer
		self.executor.lock().execute(SynchronizationTask::RequestBlocksHeaders(peer_index));
	}

	pub fn stop_sync_session(&self, peer_index: usize) {
		trace!(target: "sync", "Stopping sync session with peer#{}", peer_index);

		self.executor.lock().remove_peer_connection(peer_index);
		self.client.lock().on_peer_disconnected(peer_index);
	}

	pub fn on_peer_inventory(&self, peer_index: usize, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());

		// if there are unknown blocks => start synchronizing with peer
		let blocks_inventory = self.blocks_inventory(&message.inventory);
		if !blocks_inventory.is_empty() {
			self.client.lock().on_new_blocks_inventory(peer_index, blocks_inventory);
		}

		// if there are unknown transactions => add to memory pool
		let transactions_inventory = self.transactions_inventory(&message.inventory);
		if !transactions_inventory.is_empty() {
			self.client.lock().on_new_transactions_inventory(peer_index, transactions_inventory);
		}

		// currently we do not setup connection filter => skip InventoryType::MessageFilteredBlock
		// currently we do not send sendcmpct message => skip InventoryType::MessageCompactBlock
	}

	pub fn on_peer_getdata(&self, peer_index: usize, message: types::GetData) {
		trace!(target: "sync", "Got `getdata` message from peer#{}", peer_index);

		let filtered_inventory = {
			let mut client = self.client.lock();
			client.filter_getdata_inventory(peer_index, message.inventory)
		};
		self.server.serve_getdata(peer_index, filtered_inventory).map(|t| self.server.add_task(peer_index, t));
	}

	pub fn on_peer_getblocks(&self, peer_index: usize, message: types::GetBlocks) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);

		self.server.serve_getblocks(peer_index, message).map(|t| self.server.add_task(peer_index, t));
	}

	pub fn on_peer_getheaders(&self, peer_index: usize, message: types::GetHeaders, id: u32) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);

		// do not serve getheaders requests until we are synchronized
		let mut client = self.client.lock();
		if client.state().is_synchronizing() {
			self.executor.lock().execute(SynchronizationTask::Ignore(peer_index, id));
			return;
		}

		// simulating bitcoind for passing tests: if we are in nearly-saturated state
		// and peer, which has just provided a new blocks to us, is asking for headers
		// => do not serve getheaders until we have fully process his blocks + wait until headers are served before returning
		self.server.serve_getheaders(peer_index, message, Some(id))
			.map(|task| {
				let weak_server = Arc::downgrade(&self.server);
				let task = task.future::<U>(peer_index, weak_server);
				client.after_peer_nearly_blocks_verified(peer_index, Box::new(task));
			});
	}

	pub fn on_peer_transaction(&self, peer_index: usize, message: types::Tx) {
		trace!(target: "sync", "Got `transaction` message from peer#{}. Transaction hash: {}", peer_index, message.transaction.hash().to_reversed_str());

		// try to process new transaction
		self.client.lock().on_peer_transaction(peer_index, message.transaction);
	}

	pub fn on_peer_block(&self, peer_index: usize, message: types::Block) {
		trace!(target: "sync", "Got `block` message from peer#{}. Block hash: {}", peer_index, message.block.hash().to_reversed_str());

		// try to process new block
		self.client.lock().on_peer_block(peer_index, message.block.into());
	}

	pub fn on_peer_headers(&self, peer_index: usize, message: types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}. # of headers: {}", peer_index, message.headers.len());

		if !message.headers.is_empty() {
			self.client.lock().on_new_blocks_headers(peer_index, message.headers);
		}
	}

	pub fn on_peer_mempool(&self, peer_index: usize, _message: types::MemPool) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);

		self.server.serve_mempool(peer_index).map(|t| self.server.add_task(peer_index, t));
	}

	pub fn on_peer_filterload(&self, peer_index: usize, message: types::FilterLoad) {
		trace!(target: "sync", "Got `filterload` message from peer#{}", peer_index);
		self.client.lock().on_peer_filterload(peer_index, &message);
	}

	pub fn on_peer_filteradd(&self, peer_index: usize, message: types::FilterAdd) {
		trace!(target: "sync", "Got `filteradd` message from peer#{}", peer_index);
		self.client.lock().on_peer_filteradd(peer_index, &message);
	}

	pub fn on_peer_filterclear(&self, peer_index: usize, _message: types::FilterClear) {
		trace!(target: "sync", "Got `filterclear` message from peer#{}", peer_index);
		self.client.lock().on_peer_filterclear(peer_index);
	}

	pub fn on_peer_merkleblock(&self, peer_index: usize, _message: types::MerkleBlock) {
		trace!(target: "sync", "Got `merkleblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_sendheaders(&self, peer_index: usize, _message: types::SendHeaders) {
		trace!(target: "sync", "Got `sendheaders` message from peer#{}", peer_index);
		self.client.lock().on_peer_block_announcement_type(peer_index, BlockAnnouncementType::SendHeader);
	}

	pub fn on_peer_feefilter(&self, peer_index: usize, message: types::FeeFilter) {
		trace!(target: "sync", "Got `feefilter` message from peer#{}", peer_index);
		self.client.lock().on_peer_feefilter(peer_index, &message);
	}

	pub fn on_peer_send_compact(&self, peer_index: usize, message: types::SendCompact) {
		trace!(target: "sync", "Got `sendcmpct` message from peer#{}", peer_index);

		// The second integer SHALL be interpreted as a little-endian version number. Nodes sending a sendcmpct message MUST currently set this value to 1.
		// TODO: version 2 supports segregated witness transactions
		if message.second != 1 {
			return;
		}

		// Upon receipt of a "sendcmpct" message with the first and second integers set to 1, the node SHOULD announce new blocks by sending a cmpctblock message.
		if message.first {
			self.client.lock().on_peer_block_announcement_type(peer_index, BlockAnnouncementType::SendCompactBlock);
		}
		// else:
		// Upon receipt of a "sendcmpct" message with the first integer set to 0, the node SHOULD NOT announce new blocks by sending a cmpctblock message,
		// but SHOULD announce new blocks by sending invs or headers, as defined by BIP130.
		// => work as before
	}

	pub fn on_peer_compact_block(&self, peer_index: usize, _message: types::CompactBlock) {
		trace!(target: "sync", "Got `cmpctblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_get_block_txn(&self, peer_index: usize, message: types::GetBlockTxn) {
		trace!(target: "sync", "Got `getblocktxn` message from peer#{}", peer_index);

		// Upon receipt of a properly-formatted getblocktxn message, nodes which *recently provided the sender
		// of such a message a cmpctblock for the block hash identified in this message* MUST respond ...
		// => we should check if we have send cmpctblock before
		if {
			let mut client = self.client.lock();
			client.is_compact_block_sent_recently(peer_index, &message.request.blockhash)
		} {
			self.server.serve_get_block_txn(peer_index, message.request.blockhash, message.request.indexes).map(|t| self.server.add_task(peer_index, t));
		}
	}

	pub fn on_peer_block_txn(&self, peer_index: usize, _message: types::BlockTxn) {
		trace!(target: "sync", "Got `blocktxn` message from peer#{}", peer_index);
	}

	pub fn on_peer_notfound(&self, peer_index: usize, message: types::NotFound) {
		trace!(target: "sync", "Got `notfound` message from peer#{}", peer_index);

		let blocks_inventory = self.blocks_inventory(&message.inventory);
		self.client.lock().on_peer_blocks_notfound(peer_index, blocks_inventory);
	}

	pub fn accept_transaction(&self, transaction: Transaction) -> Result<H256, String> {
		let sink_data = Arc::new(TransactionAcceptSinkData::default());
		let sink = TransactionAcceptSink::new(sink_data.clone()).boxed();
		{
			let mut client = self.client.lock();
			if let Err(err) = client.accept_transaction(transaction, sink) {
				return Err(err.into());
			}
		}
		sink_data.wait()
	}

	fn transactions_inventory(&self, inventory: &[InventoryVector]) -> Vec<H256> {
		inventory.iter()
			.filter(|item| item.inv_type == InventoryType::MessageTx)
			.map(|item| item.hash.clone())
			.collect()
	}

	fn blocks_inventory(&self, inventory: &[InventoryVector]) -> Vec<H256> {
		inventory.iter()
			.filter(|item| item.inv_type == InventoryType::MessageBlock)
			.map(|item| item.hash.clone())
			.collect()
	}
}

impl TransactionAcceptSink {
	pub fn new(data: Arc<TransactionAcceptSinkData>) -> Self {
		TransactionAcceptSink {
			data: data,
		}
	}

	pub fn boxed(self) -> Box<Self> {
		Box::new(self)
	}
}

impl TransactionAcceptSinkData {
	pub fn wait(&self) -> Result<H256, String> {
		let mut lock = self.result.lock();
		if lock.is_some() {
			return lock.take().unwrap();
		}

		self.waiter.wait(&mut lock);
		return lock.take().unwrap();
	}
}

impl TransactionVerificationSink for TransactionAcceptSink {
	fn on_transaction_verification_success(&self, tx: Transaction) {
		*self.data.result.lock() = Some(Ok(tx.hash()));
		self.data.waiter.notify_all();
	}

	fn on_transaction_verification_error(&self, err: &str, _hash: &H256) {
		*self.data.result.lock() = Some(Err(err.to_owned()));
		self.data.waiter.notify_all();
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use parking_lot::{Mutex, RwLock};
	use connection_filter::tests::{default_filterload, make_filteradd};
	use synchronization_executor::Task;
	use synchronization_executor::tests::DummyTaskExecutor;
	use synchronization_client::{Config, SynchronizationClient, SynchronizationClientCore, CoreVerificationSink, FilteredInventory};
	use synchronization_chain::Chain;
	use p2p::{event_loop, OutboundSyncConnection, OutboundSyncConnectionRef};
	use message::types;
	use message::common::{InventoryVector, InventoryType, BlockTransactionsRequest};
	use network::Magic;
	use chain::Transaction;
	use db;
	use super::LocalNode;
	use test_data;
	use synchronization_server::ServerTask;
	use synchronization_server::tests::DummyServer;
	use synchronization_verifier::tests::DummyVerifier;
	use tokio_core::reactor::{Core, Handle};
	use primitives::bytes::Bytes;
	use verification::ChainVerifier;

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
		fn respond_headers(&self, _message: &types::Headers, _id: u32) {}
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
		fn send_notfound(&self, _message: &types::NotFound) {}
		fn ignored(&self, _id: u32) {}
		fn close(&self) {}
	}

	fn create_local_node(verifier: Option<DummyVerifier>) -> (Core, Handle, Arc<Mutex<DummyTaskExecutor>>, Arc<DummyServer>, LocalNode<DummyTaskExecutor, DummyServer, SynchronizationClient<DummyTaskExecutor, DummyVerifier>>) {
		let event_loop = event_loop();
		let handle = event_loop.handle();
		let chain = Arc::new(RwLock::new(Chain::new(Arc::new(db::TestStorage::with_genesis_block()))));
		let executor = DummyTaskExecutor::new();
		let server = Arc::new(DummyServer::new());
		let config = Config { threads_num: 1, close_connection_on_bad_block: true };
		let chain_verifier = Arc::new(ChainVerifier::new(chain.read().storage(), Magic::Mainnet));
		let client_core = SynchronizationClientCore::new(config, &handle, executor.clone(), chain.clone(), chain_verifier);
		let mut verifier = match verifier {
			Some(verifier) => verifier,
			None => DummyVerifier::default(),
		};
		verifier.set_sink(Arc::new(CoreVerificationSink::new(client_core.clone())));
		let client = SynchronizationClient::new(client_core, verifier);
		let local_node = LocalNode::new(server.clone(), client, executor.clone());
		(event_loop, handle, executor, server, local_node)
	}

	#[test]
	fn local_node_request_inventory_on_sync_start() {
		let (_, _, executor, _, local_node) = create_local_node(None);
		let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		// start sync session
		local_node.start_sync_session(peer_index, 0);
		// => ask for inventory
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![Task::RequestBlocksHeaders(peer_index)]);
	}

	#[test]
	fn local_node_serves_block() {
		let (_, _, _, server, local_node) = create_local_node(None);
		let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		// peer requests genesis block
		let genesis_block_hash = test_data::genesis().hash();
		let inventory = vec![
			InventoryVector {
				inv_type: InventoryType::MessageBlock,
				hash: genesis_block_hash.clone(),
			}
		];
		local_node.on_peer_getdata(peer_index, types::GetData {
			inventory: inventory.clone()
		});
		// => `getdata` is served
		let tasks = server.take_tasks();
		assert_eq!(tasks, vec![(peer_index, ServerTask::ServeGetData(FilteredInventory::with_unfiltered(inventory)))]);
	}

	#[test]
	fn local_node_serves_merkleblock() {
		let (_, _, _, server, local_node) = create_local_node(None);

		let genesis = test_data::genesis();
		let b1 = test_data::block_builder().header().parent(genesis.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b1
		let b2 = test_data::block_builder().header().parent(b1.hash()).build()
			.transaction().output().value(20).build().build()
			.build(); // genesis -> b1 -> b2
		let tx1 = b1.transactions[0].clone();
		let tx2 = b2.transactions[0].clone();
		let tx1_hash = tx1.hash();
		let tx2_hash = tx2.hash();
		let b1_hash = b1.hash();
		let b2_hash = b2.hash();
		let match_tx1 = vec![(tx1_hash.clone(), tx1)];
		let match_tx2 = vec![(tx2_hash.clone(), tx2)];
		let no_match_bytes = Bytes::from(vec![0x00]);
		let match_bytes = Bytes::from(vec![0x01]);

		// This peer will provide blocks
		let peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_block(peer_index1, types::Block { block: b1.clone() });
		local_node.on_peer_block(peer_index1, types::Block { block: b2.clone() });

		// This peer won't get any blocks, because it has not set filter for the connection
		let peer_index2 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_getdata(peer_index2, types::GetData {inventory: vec![
				InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b1_hash.clone() },
				InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b2_hash.clone() },
			]});
		assert_eq!(server.take_tasks(), vec![(peer_index2, ServerTask::ServeGetData(FilteredInventory::with_notfound(vec![
			InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b1_hash.clone() },
			InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b2_hash.clone() },
		])))]);

		let peers_config = vec![
			(true, false), // will get tx1
			(false, true), // will get tx2
			(true, true), // will get both tx
			(false, false), // won't get any tx
		];

		for (get_tx1, get_tx2) in peers_config {
			let peer_index = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
			// setup filter
			local_node.on_peer_filterload(peer_index, default_filterload());
			if get_tx1 {
				local_node.on_peer_filteradd(peer_index, make_filteradd(&*tx1_hash));
			}
			if get_tx2 {
				local_node.on_peer_filteradd(peer_index, make_filteradd(&*tx2_hash));
			}

			// ask for data
			local_node.on_peer_getdata(peer_index, types::GetData {inventory: vec![
					InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b1_hash.clone() },
					InventoryVector { inv_type: InventoryType::MessageFilteredBlock, hash: b2_hash.clone() },
				]});

			// get server tasks
			let tasks = server.take_tasks();
			assert_eq!(tasks.len(), 1);
			match tasks[0] {
				(_, ServerTask::ServeGetData(ref filtered_inventory)) => {
					assert_eq!(filtered_inventory.unfiltered.len(), 0);
					assert_eq!(filtered_inventory.notfound.len(), 0);
					assert_eq!(filtered_inventory.filtered.len(), 2);

					assert_eq!(filtered_inventory.filtered[0].0, types::MerkleBlock {
						block_header: b1.block_header.clone(),
						total_transactions: 1,
						hashes: vec![tx1_hash.clone()],
						flags: if get_tx1 { match_bytes.clone() } else { no_match_bytes.clone() },
					});
					if get_tx1 {
						assert_eq!(filtered_inventory.filtered[0].1, match_tx1);
					} else {
						assert_eq!(filtered_inventory.filtered[0].1, vec![]);
					}

					assert_eq!(filtered_inventory.filtered[1].0, types::MerkleBlock {
						block_header: b2.block_header.clone(),
						total_transactions: 1,
						hashes: vec![tx2_hash.clone()],
						flags: if get_tx2 { match_bytes.clone() } else { no_match_bytes.clone() },
					});
					if get_tx2 {
						assert_eq!(filtered_inventory.filtered[1].1, match_tx2);
					} else {
						assert_eq!(filtered_inventory.filtered[1].1, vec![]);
					}
				},
				_ => panic!("unexpected"),
			}
		}
	}

	#[test]
	fn local_node_serves_compactblock() {
		let (_, _, _, server, local_node) = create_local_node(None);

		let genesis = test_data::genesis();
		let b1 = test_data::block_builder().header().parent(genesis.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b1
		let b1_hash = b1.hash();

		// This peer will provide blocks
		let peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_block(peer_index1, types::Block { block: b1.clone() });

		// This peer will receive compact block
		let peer_index2 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_getdata(peer_index2, types::GetData {inventory: vec![
				InventoryVector { inv_type: InventoryType::MessageCompactBlock, hash: b1_hash.clone() },
			]});
		let tasks = server.take_tasks();
		assert_eq!(tasks.len(), 1);
		match tasks[0] {
			(_, ServerTask::ServeGetData(ref gd)) => {
				assert_eq!(gd.filtered.len(), 0);
				assert_eq!(gd.unfiltered.len(), 0);
				assert_eq!(gd.notfound.len(), 0);
				assert_eq!(gd.compacted.len(), 1);
			},
			_ => panic!("unexpected"),
		}
	}

	#[test]
	fn local_node_serves_get_block_txn_when_recently_sent_compact_block() {
		let (_, _, _, server, local_node) = create_local_node(None);

		let genesis = test_data::genesis();
		let b1 = test_data::block_builder().header().parent(genesis.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b1
		let b1_hash = b1.hash();

		// Append block
		let peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_block(peer_index1, types::Block { block: b1.clone() });

		// Request compact block
		let peer_index2 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_getdata(peer_index2, types::GetData {inventory: vec![
				InventoryVector { inv_type: InventoryType::MessageCompactBlock, hash: b1_hash.clone() },
			]});

		// forget tasks
		server.take_tasks();

		// Request compact transaction from this block
		local_node.on_peer_get_block_txn(peer_index2, types::GetBlockTxn {
			request: BlockTransactionsRequest {
				blockhash: b1_hash.clone(),
				indexes: vec![0],
			}
		});

		let tasks = server.take_tasks();
		assert_eq!(tasks, vec![(2, ServerTask::ServeGetBlockTxn(b1_hash, vec![0]))]);

	}

	#[test]
	fn local_node_not_serves_get_block_txn_when_compact_block_was_not_sent() {
		let (_, _, _, server, local_node) = create_local_node(None);

		let genesis = test_data::genesis();
		let b1 = test_data::block_builder().header().parent(genesis.hash()).build()
			.transaction().output().value(10).build().build()
			.build(); // genesis -> b1
		let b1_hash = b1.hash();

		// Append block
		let peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_block(peer_index1, types::Block { block: b1.clone() });

		// Request compact transaction from this block
		let peer_index2 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		local_node.on_peer_get_block_txn(peer_index2, types::GetBlockTxn {
			request: BlockTransactionsRequest {
				blockhash: b1_hash,
				indexes: vec![0],
			}
		});

		let tasks = server.take_tasks();
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn local_node_accepts_local_transaction() {
		let (_, _, executor, _, local_node) = create_local_node(None);

		// transaction will be relayed to this peer
		let peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		{ executor.lock().take_tasks(); }

		let genesis = test_data::genesis();
		let transaction: Transaction = test_data::TransactionBuilder::with_output(1).add_input(&genesis.transactions[0], 0).into();
		let transaction_hash = transaction.hash();

		let result = local_node.accept_transaction(transaction);
		assert_eq!(result, Ok(transaction_hash));

		assert_eq!(executor.lock().take_tasks(), vec![Task::SendInventory(peer_index1,
			vec![InventoryVector {
					inv_type: InventoryType::MessageTx,
					hash: "0791efccd035c5fe501023ff888106eba5eff533965de4a6e06400f623bcac34".into(),
				}]
			)]
		);
	}

	#[test]
	fn local_node_discards_local_transaction() {
		let genesis = test_data::genesis();
		let transaction: Transaction = test_data::TransactionBuilder::with_output(1).add_input(&genesis.transactions[0], 0).into();
		let transaction_hash = transaction.hash();

		// simulate transaction verification fail
		let mut verifier = DummyVerifier::default();
		verifier.error_when_verifying(transaction_hash.clone(), "simulated");

		let (_, _, executor, _, local_node) = create_local_node(Some(verifier));

		let _peer_index1 = local_node.create_sync_session(0, DummyOutboundSyncConnection::new());
		{ executor.lock().take_tasks(); }

		let result = local_node.accept_transaction(transaction);
		assert_eq!(result, Err("simulated".to_owned()));

		assert_eq!(executor.lock().take_tasks(), vec![]);
	}
}
