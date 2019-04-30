use std::sync::Arc;
use chain::{IndexedBlock, IndexedTransaction};
use message::common::InventoryVector;
use message::types;
use primitives::hash::H256;
use synchronization_peers::{BlockAnnouncementType, TransactionAnnouncementType};
use types::{PeerIndex, PeersRef, RequestId};
use utils::KnownHashType;

/// Synchronization task executor
pub trait TaskExecutor : Send + Sync + 'static {
	fn execute(&self, task: Task);
}

/// Synchronization task for the peer.
#[derive(Debug, PartialEq)]
pub enum Task {
	/// Notify io about ignored request
	Ignore(PeerIndex, RequestId),
	/// Request unknown items from peer
	GetData(PeerIndex, types::GetData),
	/// Get headers
	GetHeaders(PeerIndex, types::GetHeaders),
	/// Get memory pool
	MemoryPool(PeerIndex),
	/// Send block
	Block(PeerIndex, IndexedBlock),
	/// Send merkleblock
	MerkleBlock(PeerIndex, H256, types::MerkleBlock),
	/// Send cmpcmblock
	CompactBlock(PeerIndex, H256, types::CompactBlock),
	/// Send block with witness data
	WitnessBlock(PeerIndex, IndexedBlock),
	/// Send transaction
	Transaction(PeerIndex, IndexedTransaction),
	/// Send transaction with witness data
	WitnessTransaction(PeerIndex, IndexedTransaction),
	/// Send block transactions
	BlockTxn(PeerIndex, types::BlockTxn),
	/// Send notfound
	NotFound(PeerIndex, types::NotFound),
	/// Send inventory
	Inventory(PeerIndex, types::Inv),
	/// Send headers
	Headers(PeerIndex, types::Headers, Option<RequestId>),
	/// Relay new block to peers
	RelayNewBlock(IndexedBlock),
	/// Relay new transaction to peers
	RelayNewTransaction(IndexedTransaction, u64),
}

/// Synchronization tasks executor
pub struct LocalSynchronizationTaskExecutor {
	/// Active synchronization peers
	peers: PeersRef,
}

impl LocalSynchronizationTaskExecutor {
	pub fn new(peers: PeersRef) -> Arc<Self> {
		Arc::new(LocalSynchronizationTaskExecutor {
			peers: peers,
		})
	}

	fn execute_ignore(&self, peer_index: PeerIndex, request_id: RequestId) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Ignoring request {} from peer#{}", request_id, peer_index);
			connection.ignored(request_id);
		}
	}

	fn execute_getdata(&self, peer_index: PeerIndex, getdata: types::GetData) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Querying {} unknown items from peer#{}", getdata.inventory.len(), peer_index);
			connection.send_getdata(&getdata);
		}
	}

	fn execute_getheaders(&self, peer_index: PeerIndex, getheaders: types::GetHeaders) {
		if let Some(connection) = self.peers.connection(peer_index) {
			if !getheaders.block_locator_hashes.is_empty() {
				trace!(target: "sync", "Querying headers starting with {} unknown items from peer#{}", getheaders.block_locator_hashes[0].to_reversed_str(), peer_index);
			}
			connection.send_getheaders(&getheaders);
		}
	}

	fn execute_memorypool(&self, peer_index: PeerIndex) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Querying memory pool contents from peer#{}", peer_index);
			let mempool = types::MemPool;
			connection.send_mempool(&mempool);
		}
	}

	fn execute_block(&self, peer_index: PeerIndex, block: IndexedBlock) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending block {} to peer#{}", block.hash().to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, block.hash().clone(), KnownHashType::Block);
			let block = types::Block {
				block: block.to_raw_block(),
			};
			connection.send_block(&block);
		}
	}

	fn execute_merkleblock(&self, peer_index: PeerIndex, hash: H256, block: types::MerkleBlock) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending merkle block {} to peer#{}", hash.to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, hash, KnownHashType::Block);
			connection.send_merkleblock(&block);
		}
	}

	fn execute_compact_block(&self, peer_index: PeerIndex, hash: H256, block: types::CompactBlock) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending compact block {} to peer#{}", hash.to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, hash, KnownHashType::CompactBlock);
			connection.send_compact_block(&block);
		}
	}

	fn execute_witness_block(&self, peer_index: PeerIndex, block: IndexedBlock) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending witness block {} to peer#{}", block.hash().to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, block.hash().clone(), KnownHashType::Block);
			let block = types::Block {
				block: block.to_raw_block(),
			};
			connection.send_witness_block(&block);
		}
	}

	fn execute_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending transaction {} to peer#{}", transaction.hash.to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, transaction.hash, KnownHashType::Transaction);
			let transaction = types::Tx {
				transaction: transaction.raw,
			};
			connection.send_transaction(&transaction);
		}
	}

	fn execute_witness_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending witness transaction {} to peer#{}", transaction.hash.to_reversed_str(), peer_index);
			self.peers.hash_known_as(peer_index, transaction.hash, KnownHashType::Transaction);
			let transaction = types::Tx {
				transaction: transaction.raw,
			};
			connection.send_witness_transaction(&transaction);
		}
	}

	fn execute_block_txn(&self, peer_index: PeerIndex, blocktxn: types::BlockTxn) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending blocktxn with {} transactions to peer#{}", blocktxn.request.transactions.len(), peer_index);
			connection.send_block_txn(&blocktxn);
		}
	}

	fn execute_notfound(&self, peer_index: PeerIndex, notfound: types::NotFound) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending notfound to peer#{} with {} items", peer_index, notfound.inventory.len());
			connection.send_notfound(&notfound);
		}
	}

	fn execute_inventory(&self, peer_index: PeerIndex, inventory: types::Inv) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending inventory to peer#{} with {} items", peer_index, inventory.inventory.len());
			connection.send_inventory(&inventory);
		}
	}

	fn execute_headers(&self, peer_index: PeerIndex, headers: types::Headers, request_id: Option<RequestId>) {
		if let Some(connection) = self.peers.connection(peer_index) {
			trace!(target: "sync", "Sending headers to peer#{} with {} items", peer_index, headers.headers.len());
			match request_id {
				Some(request_id) => connection.respond_headers(&headers, request_id),
				None => connection.send_headers(&headers),
			}
		}
	}

	fn execute_relay_block(&self, block: IndexedBlock) {
		for peer_index in self.peers.enumerate() {
			match self.peers.filter_block(peer_index, &block) {
				BlockAnnouncementType::SendInventory => {
					self.execute_inventory(peer_index, types::Inv::with_inventory(vec![
						InventoryVector::block(block.hash().clone()),
					]));
				},
				BlockAnnouncementType::SendHeaders => {
					self.execute_headers(peer_index, types::Headers::with_headers(vec![
						block.header.raw.clone(),
					]), None);
				},
				BlockAnnouncementType::SendCompactBlock => if let Some(compact_block) = self.peers.build_compact_block(peer_index, &block) {
					self.execute_compact_block(peer_index, *block.hash(), compact_block);
				},
				BlockAnnouncementType::DoNotAnnounce => (),
			}
		}
	}

	fn execute_relay_transaction(&self, transaction: IndexedTransaction, fee_rate: u64) {
		for peer_index in self.peers.enumerate() {
			match self.peers.filter_transaction(peer_index, &transaction, Some(fee_rate)) {
				TransactionAnnouncementType::SendInventory => self.execute_inventory(peer_index, types::Inv::with_inventory(vec![
					InventoryVector::tx(transaction.hash.clone()),
				])),
				TransactionAnnouncementType::DoNotAnnounce => (),
			}
		}
	}
}

impl TaskExecutor for LocalSynchronizationTaskExecutor {
	fn execute(&self, task: Task) {
		match task {
			Task::Ignore(peer_index, request_id) => self.execute_ignore(peer_index, request_id),
			Task::GetData(peer_index, getdata) => self.execute_getdata(peer_index, getdata),
			Task::GetHeaders(peer_index, getheaders) => self.execute_getheaders(peer_index, getheaders),
			Task::MemoryPool(peer_index) => self.execute_memorypool(peer_index),
			Task::Block(peer_index, block) => self.execute_block(peer_index, block),
			Task::MerkleBlock(peer_index, hash, block) => self.execute_merkleblock(peer_index, hash, block),
			Task::CompactBlock(peer_index, hash, block) => self.execute_compact_block(peer_index, hash, block),
			Task::WitnessBlock(peer_index, block) => self.execute_witness_block(peer_index, block),
			Task::Transaction(peer_index, transaction) => self.execute_transaction(peer_index, transaction),
			Task::WitnessTransaction(peer_index, transaction) => self.execute_witness_transaction(peer_index, transaction),
			Task::BlockTxn(peer_index, blocktxn) => self.execute_block_txn(peer_index, blocktxn),
			Task::NotFound(peer_index, notfound) => self.execute_notfound(peer_index, notfound),
			Task::Inventory(peer_index, inventory) => self.execute_inventory(peer_index, inventory),
			Task::Headers(peer_index, headers, request_id) => self.execute_headers(peer_index, headers, request_id),
			Task::RelayNewBlock(block) => self.execute_relay_block(block),
			Task::RelayNewTransaction(transaction, fee_rate) => self.execute_relay_transaction(transaction, fee_rate),
		}
	}
}

#[cfg(test)]
pub mod tests {
	extern crate test_data;

	use super::*;
	use std::sync::Arc;
	use std::time;
	use parking_lot::{Mutex, Condvar};
	use chain::Transaction;
	use message::{Services, types};
	use inbound_connection::tests::DummyOutboundSyncConnection;
	use local_node::tests::{default_filterload, make_filteradd};
	use synchronization_peers::{PeersImpl, PeersContainer, PeersFilters, PeersOptions, BlockAnnouncementType};

	pub struct DummyTaskExecutor {
		tasks: Mutex<Vec<Task>>,
		waiter: Arc<Condvar>,
	}

	impl DummyTaskExecutor {
		pub fn new() -> Arc<Self> {
			Arc::new(DummyTaskExecutor {
				tasks: Mutex::new(Vec::new()),
				waiter: Arc::new(Condvar::new()),
			})
		}

		pub fn wait_tasks_for(executor: Arc<Self>, timeout_ms: u64) -> Vec<Task> {
			{
				let mut tasks = executor.tasks.lock();
				if tasks.is_empty() {
					let waiter = executor.waiter.clone();
					waiter.wait_for(&mut tasks, time::Duration::from_millis(timeout_ms)).timed_out();
				}
			}
			executor.take_tasks()
		}

		pub fn wait_tasks(executor: Arc<Self>) -> Vec<Task> {
			DummyTaskExecutor::wait_tasks_for(executor, 1000)
		}

		pub fn take_tasks(&self) -> Vec<Task> {
			let mut tasks = self.tasks.lock();
			let tasks = tasks.drain(..).collect();
			tasks
		}
	}

	impl TaskExecutor for DummyTaskExecutor {
		fn execute(&self, task: Task) {
			self.tasks.lock().push(task);
			self.waiter.notify_one();
		}
	}

	#[test]
	fn relay_new_block_after_sendcmpct() {
		let peers = Arc::new(PeersImpl::default());
		let executor = LocalSynchronizationTaskExecutor::new(peers.clone());

		let c1 = DummyOutboundSyncConnection::new();
		peers.insert(1, Services::default(), c1.clone());
		let c2 = DummyOutboundSyncConnection::new();
		peers.insert(2, Services::default(), c2.clone());
		peers.set_block_announcement_type(2, BlockAnnouncementType::SendCompactBlock);

		executor.execute(Task::RelayNewBlock(test_data::genesis().into()));
		assert_eq!(*c1.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c2.messages.lock().entry("cmpctblock".to_owned()).or_insert(0), 1);
	}

	#[test]
	fn relay_new_block_after_sendheaders() {
		let peers = Arc::new(PeersImpl::default());
		let executor = LocalSynchronizationTaskExecutor::new(peers.clone());

		let c1 = DummyOutboundSyncConnection::new();
		peers.insert(1, Services::default(), c1.clone());
		let c2 = DummyOutboundSyncConnection::new();
		peers.insert(2, Services::default(), c2.clone());
		peers.set_block_announcement_type(2, BlockAnnouncementType::SendHeaders);

		executor.execute(Task::RelayNewBlock(test_data::genesis().into()));
		assert_eq!(*c1.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c2.messages.lock().entry("headers".to_owned()).or_insert(0), 1);
	}

	#[test]
	fn relay_new_transaction_with_bloom_filter() {
		let peers = Arc::new(PeersImpl::default());
		let executor = LocalSynchronizationTaskExecutor::new(peers.clone());

		let tx1: Transaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_output(20).into();
		let tx3: Transaction = test_data::TransactionBuilder::with_output(30).into();
		let tx1_hash = tx1.hash();
		let tx2_hash = tx2.hash();
		let tx3_hash = tx3.hash();

		// peer#1 wants tx1
		let c1 = DummyOutboundSyncConnection::new();
		peers.insert(1, Services::default(), c1.clone());
		peers.set_bloom_filter(1, default_filterload());
		peers.update_bloom_filter(1, make_filteradd(&*tx1_hash));
		// peer#2 wants tx2
		let c2 = DummyOutboundSyncConnection::new();
		peers.insert(2, Services::default(), c2.clone());
		peers.set_bloom_filter(2, default_filterload());
		peers.update_bloom_filter(2, make_filteradd(&*tx2_hash));
		// peer#3 wants tx1 + tx2 transactions
		let c3 = DummyOutboundSyncConnection::new();
		peers.insert(3, Services::default(), c3.clone());
		peers.set_bloom_filter(3, default_filterload());
		peers.update_bloom_filter(3, make_filteradd(&*tx1_hash));
		peers.update_bloom_filter(3, make_filteradd(&*tx2_hash));
		// peer#4 has default behaviour (no filter)
		let c4 = DummyOutboundSyncConnection::new();
		peers.insert(4, Services::default(), c4.clone());
		// peer#5 wants some other transactions
		let c5 = DummyOutboundSyncConnection::new();
		peers.insert(5, Services::default(), c5.clone());
		peers.set_bloom_filter(5, default_filterload());
		peers.update_bloom_filter(5, make_filteradd(&*tx3_hash));

		// tx1 is relayed to peers: 1, 3, 4
		executor.execute(Task::RelayNewTransaction(tx1.into(), 0));

		assert_eq!(*c1.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c2.messages.lock().entry("inventory".to_owned()).or_insert(0), 0);
		assert_eq!(*c3.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c4.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);

		// tx2 is relayed to peers: 2, 3, 4
		executor.execute(Task::RelayNewTransaction(tx2.into(), 0));

		assert_eq!(*c1.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c2.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c3.messages.lock().entry("inventory".to_owned()).or_insert(0), 2);
		assert_eq!(*c4.messages.lock().entry("inventory".to_owned()).or_insert(0), 2);
	}

	#[test]
	fn relay_new_transaction_with_feefilter() {
		let peers = Arc::new(PeersImpl::default());
		let executor = LocalSynchronizationTaskExecutor::new(peers.clone());

		let c2 = DummyOutboundSyncConnection::new();
		peers.insert(2, Services::default(), c2.clone());
		peers.set_fee_filter(2, types::FeeFilter::with_fee_rate(3000));
		let c3 = DummyOutboundSyncConnection::new();
		peers.insert(3, Services::default(), c3.clone());
		peers.set_fee_filter(3, types::FeeFilter::with_fee_rate(4000));
		let c4 = DummyOutboundSyncConnection::new();
		peers.insert(4, Services::default(), c4.clone());

		executor.execute(Task::RelayNewTransaction(test_data::genesis().transactions[0].clone().into(), 3500));

		assert_eq!(*c2.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
		assert_eq!(*c3.messages.lock().entry("inventory".to_owned()).or_insert(0), 0);
		assert_eq!(*c4.messages.lock().entry("inventory".to_owned()).or_insert(0), 1);
	}
}
