use std::sync::Arc;
use parking_lot::{Mutex, Condvar};
use time;
use futures::{lazy, finished};
use chain::{IndexedTransaction, IndexedBlock, IndexedBlockHeader};
use message::types;
use miner::BlockAssembler;
use network::ConsensusParams;
use synchronization_client::{Client};
use synchronization_server::{Server, ServerTask};
use synchronization_verifier::{TransactionVerificationSink};
use primitives::hash::H256;
use miner::BlockTemplate;
use verification::median_timestamp_inclusive;
use synchronization_peers::{TransactionAnnouncementType, BlockAnnouncementType};
use types::{PeerIndex, RequestId, StorageRef, MemoryPoolRef, PeersRef,
	ClientRef, ServerRef, SynchronizationStateRef, SyncListenerRef};

/// Local synchronization node
pub struct LocalNode<U: Server, V: Client> {
	/// Network we are working on
	consensus: ConsensusParams,
	/// Storage reference
	storage: StorageRef,
	/// Memory pool reference
	memory_pool: MemoryPoolRef,
	/// Synchronization peers
	peers: PeersRef,
	/// Shared synchronization state
	state: SynchronizationStateRef,
	/// Synchronization process
	client: ClientRef<V>,
	/// Synchronization server
	server: ServerRef<U>,
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

impl<U, V> LocalNode<U, V> where U: Server, V: Client {
	/// Create new synchronization node
	#[cfg_attr(feature="cargo-clippy", allow(too_many_arguments))]
	pub fn new(consensus: ConsensusParams, storage: StorageRef, memory_pool: MemoryPoolRef, peers: PeersRef,
		state: SynchronizationStateRef, client: ClientRef<V>, server: ServerRef<U>) -> Self {
		LocalNode {
			consensus: consensus,
			storage: storage,
			memory_pool: memory_pool,
			peers: peers,
			state: state,
			client: client,
			server: server,
		}
	}

	/// Return shared reference to synchronization state.
	pub fn sync_state(&self) -> SynchronizationStateRef {
		self.state.clone()
	}

	/// When new peer connects to the node
	pub fn on_connect(&self, peer_index: PeerIndex, peer_name: String, version: types::Version) {
		trace!(target: "sync", "Starting new sync session with peer#{}: {}", peer_index, peer_name);

		// light clients may not want transactions broadcasting until filter for connection is set
		if !version.relay_transactions() {
			self.peers.set_transaction_announcement_type(peer_index, TransactionAnnouncementType::DoNotAnnounce);
		}

		// start synchronization session with peer
		self.client.on_connect(peer_index);
	}

	/// When peer disconnects
	pub fn on_disconnect(&self, peer_index: PeerIndex) {
		trace!(target: "sync", "Stopping sync session with peer#{}", peer_index);

		// stop synchronization session with peer
		self.client.on_disconnect(peer_index);
	}

	/// When inventory message is received
	pub fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());
		self.client.on_inventory(peer_index, message);
	}

	/// When headers message is received
	pub fn on_headers(&self, peer_index: PeerIndex, headers: Vec<IndexedBlockHeader>) {
		trace!(target: "sync", "Got `headers` message from peer#{}. Headers len: {}", peer_index, headers.len());
		self.client.on_headers(peer_index, headers);
	}

	/// When transaction is received
	pub fn on_transaction(&self, peer_index: PeerIndex, tx: IndexedTransaction) {
		trace!(target: "sync", "Got `transaction` message from peer#{}. Tx hash: {}", peer_index, tx.hash.to_reversed_str());
		self.client.on_transaction(peer_index, tx);
	}

	/// When block is received
	pub fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock) {
		trace!(target: "sync", "Got `block` message from peer#{}. Block hash: {}", peer_index, block.header.hash.to_reversed_str());
		self.client.on_block(peer_index, block);
	}

	/// When notfound is received
	pub fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound) {
		trace!(target: "sync", "Got `notfound` message from peer#{}", peer_index);
		self.client.on_notfound(peer_index, message);
	}

	/// When peer is requesting for items
	pub fn on_getdata(&self, peer_index: PeerIndex, message: types::GetData) {
		trace!(target: "sync", "Got `getdata` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());
		self.server.execute(ServerTask::GetData(peer_index, message));
	}

	/// When peer is requesting for known blocks hashes
	pub fn on_getblocks(&self, peer_index: PeerIndex, message: types::GetBlocks) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);
		self.server.execute(ServerTask::GetBlocks(peer_index, message));
	}

	/// When peer is requesting for known blocks headers
	pub fn on_getheaders(&self, peer_index: PeerIndex, message: types::GetHeaders, id: RequestId) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);

		// simulating bitcoind for passing tests: if we are in nearly-saturated state
		// and peer, which has just provided a new blocks to us, is asking for headers
		// => do not serve getheaders until we have fully process his blocks + wait until headers are served before returning
		let server = Arc::downgrade(&self.server);
		let server_task = ServerTask::GetHeaders(peer_index, message, id);
		let lazy_server_task = lazy(move || {
			server.upgrade().map(|s| s.execute(server_task));
			finished::<(), ()>(())
		});
		self.client.after_peer_nearly_blocks_verified(peer_index, Box::new(lazy_server_task));
	}

	/// When peer is requesting for memory pool contents
	pub fn on_mempool(&self, peer_index: PeerIndex, _message: types::MemPool) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);
		self.server.execute(ServerTask::Mempool(peer_index));
	}

	/// When peer asks us from specific transactions from specific block
	pub fn on_get_block_txn(&self, peer_index: PeerIndex, message: types::GetBlockTxn) {
		if self.state.synchronizing() {
			trace!(target: "sync", "Ignored `getblocktxn` message from peer#{}", peer_index);
			return;
		}

		trace!(target: "sync", "Got `getblocktxn` message from peer#{}", peer_index);
		self.server.execute(ServerTask::GetBlockTxn(peer_index, message));
	}

	/// When peer sets bloom filter for connection
	pub fn on_filterload(&self, peer_index: PeerIndex, message: types::FilterLoad) {
		trace!(target: "sync", "Got `filterload` message from peer#{}", peer_index);
		self.peers.set_bloom_filter(peer_index, message);
	}

	/// When peer updates bloom filter for connection
	pub fn on_filteradd(&self, peer_index: PeerIndex, message: types::FilterAdd) {
		trace!(target: "sync", "Got `filteradd` message from peer#{}", peer_index);
		self.peers.update_bloom_filter(peer_index, message);
	}

	/// When peer removes bloom filter from connection
	pub fn on_filterclear(&self, peer_index: PeerIndex, _message: types::FilterClear) {
		trace!(target: "sync", "Got `filterclear` message from peer#{}", peer_index);
		self.peers.clear_bloom_filter(peer_index);
	}

	/// When peer sets up a minimum fee rate filter for connection
	pub fn on_feefilter(&self, peer_index: PeerIndex, message: types::FeeFilter) {
		trace!(target: "sync", "Got `feefilter` message from peer#{}", peer_index);
		self.peers.set_fee_filter(peer_index, message);
	}

	/// When peer asks us to announce new blocks using headers message
	pub fn on_sendheaders(&self, peer_index: PeerIndex, _message: types::SendHeaders) {
		trace!(target: "sync", "Got `sendheaders` message from peer#{}", peer_index);
		self.peers.set_block_announcement_type(peer_index, BlockAnnouncementType::SendHeaders);
	}

	/// When peer asks us to announce new blocks using cpmctblock message
	pub fn on_send_compact(&self, peer_index: PeerIndex, message: types::SendCompact) {
		trace!(target: "sync", "Got `sendcmpct` message from peer#{}", peer_index);

		// The second integer SHALL be interpreted as a little-endian version number. Nodes sending a sendcmpct message MUST currently set this value to 1.
		// TODO: version 2 supports segregated witness transactions
		if message.second != 1 {
			return;
		}

		// Upon receipt of a "sendcmpct" message with the first and second integers set to 1, the node SHOULD announce new blocks by sending a cmpctblock message.
		if message.first {
			self.peers.set_block_announcement_type(peer_index, BlockAnnouncementType::SendCompactBlock);
		}

		// else:
		// Upon receipt of a "sendcmpct" message with the first integer set to 0, the node SHOULD NOT announce new blocks by sending a cmpctblock message,
		// but SHOULD announce new blocks by sending invs or headers, as defined by BIP130.
		// => work as before
	}

	/// When peer sents us a merkle block
	pub fn on_merkleblock(&self, peer_index: PeerIndex, _message: types::MerkleBlock) {
		trace!(target: "sync", "Got `merkleblock` message from peer#{}", peer_index);
		// we never setup filter on connections => misbehaving
		self.peers.misbehaving(peer_index, "Got unrequested 'merkleblock' message");
	}

	/// When peer sents us a compact block
	pub fn on_compact_block(&self, peer_index: PeerIndex, _message: types::CompactBlock) {
		trace!(target: "sync", "Got `cmpctblock` message from peer#{}", peer_index);
		// we never ask compact block from peers => misbehaving
		self.peers.misbehaving(peer_index, "Got unrequested 'cmpctblock' message");
	}

	/// When peer sents us specific transactions for specific block
	pub fn on_block_txn(&self, peer_index: PeerIndex, _message: types::BlockTxn) {
		trace!(target: "sync", "Got `blocktxn` message from peer#{}", peer_index);
		// we never ask for this => misbehaving
		self.peers.misbehaving(peer_index, "Got unrequested 'blocktxn' message");
	}

	/// Verify and then schedule new transaction
	pub fn accept_transaction(&self, transaction: IndexedTransaction) -> Result<H256, String> {
		let sink_data = Arc::new(TransactionAcceptSinkData::default());
		let sink = TransactionAcceptSink::new(sink_data.clone()).boxed();
		{
			if let Err(err) = self.client.accept_transaction(transaction, sink) {
				return Err(err.into());
			}
		}
		sink_data.wait()
	}

	/// Get block template for mining
	pub fn get_block_template(&self) -> BlockTemplate {
		let previous_block_height = self.storage.best_block().number;
		let previous_block_header = self.storage.block_header(previous_block_height.into()).expect("best block is in db; qed");
		let median_timestamp = median_timestamp_inclusive(previous_block_header.hash, self.storage.as_block_header_provider());
		let new_block_height = previous_block_height + 1;
		let max_block_size = self.consensus.fork.max_block_size(new_block_height, median_timestamp);
		let block_assembler = BlockAssembler {
			max_block_size: max_block_size as u32,
			max_block_sigops: self.consensus.fork.max_block_sigops(new_block_height, max_block_size) as u32,
		};
		let memory_pool = &*self.memory_pool.read();
		block_assembler.create_new_block(&self.storage, memory_pool, time::get_time().sec as u32, median_timestamp, &self.consensus)
	}

	/// Install synchronization events listener
	pub fn install_sync_listener(&self, listener: SyncListenerRef) {
		self.client.install_sync_listener(listener);
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
			return lock.take().expect("checked line above");
		}

		self.waiter.wait(&mut lock);
		lock.take().expect("waiter.wait returns only when result is set; lock.take() takes result from waiter.result; qed")
	}
}

impl TransactionVerificationSink for TransactionAcceptSink {
	fn on_transaction_verification_success(&self, tx: IndexedTransaction) {
		*self.data.result.lock() = Some(Ok(tx.hash));
		self.data.waiter.notify_all();
	}

	fn on_transaction_verification_error(&self, err: &str, _hash: &H256) {
		*self.data.result.lock() = Some(Err(err.to_owned()));
		self.data.waiter.notify_all();
	}
}

#[cfg(test)]
pub mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use parking_lot::RwLock;
	use synchronization_executor::Task;
	use synchronization_executor::tests::DummyTaskExecutor;
	use synchronization_client::SynchronizationClient;
	use synchronization_client_core::{Config, SynchronizationClientCore, CoreVerificationSink};
	use synchronization_chain::Chain;
	use message::types;
	use message::common::{InventoryVector, InventoryType};
	use network::{ConsensusParams, ConsensusFork, Network};
	use chain::Transaction;
	use db::{BlockChainDatabase};
	use miner::MemoryPool;
	use super::LocalNode;
	use synchronization_server::ServerTask;
	use synchronization_server::tests::DummyServer;
	use synchronization_verifier::tests::DummyVerifier;
	use primitives::bytes::Bytes;
	use verification::BackwardsCompatibleChainVerifier as ChainVerifier;
	use std::iter::repeat;
	use synchronization_peers::PeersImpl;
	use utils::SynchronizationState;
	use types::SynchronizationStateRef;

	pub fn default_filterload() -> types::FilterLoad {
		types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		}
	}

	pub fn make_filteradd(data: &[u8]) -> types::FilterAdd {
		types::FilterAdd {
			data: data.into(),
		}
	}

	fn create_local_node(verifier: Option<DummyVerifier>) -> (Arc<DummyTaskExecutor>, Arc<DummyServer>, LocalNode<DummyServer, SynchronizationClient<DummyTaskExecutor, DummyVerifier>>) {
		let memory_pool = Arc::new(RwLock::new(MemoryPool::new()));
		let storage = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let sync_state = SynchronizationStateRef::new(SynchronizationState::with_storage(storage.clone()));
		let chain = Chain::new(storage.clone(), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore), memory_pool.clone());
		let sync_peers = Arc::new(PeersImpl::default());
		let executor = DummyTaskExecutor::new();
		let server = Arc::new(DummyServer::new());
		let config = Config { close_connection_on_bad_block: true };
		let chain_verifier = Arc::new(ChainVerifier::new(storage.clone(), ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore)));
		let client_core = SynchronizationClientCore::new(config, sync_state.clone(), sync_peers.clone(), executor.clone(), chain, chain_verifier);
		let mut verifier = match verifier {
			Some(verifier) => verifier,
			None => DummyVerifier::default(),
		};
		verifier.set_sink(Arc::new(CoreVerificationSink::new(client_core.clone())));
		let client = SynchronizationClient::new(sync_state.clone(), client_core, verifier);
		let local_node = LocalNode::new(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore), storage, memory_pool, sync_peers, sync_state, client, server.clone());
		(executor, server, local_node)
	}

	#[test]
	fn local_node_serves_block() {
		let (_, server, local_node) = create_local_node(None);
		let peer_index = 0; local_node.on_connect(peer_index, "test".into(), types::Version::default());
		// peer requests genesis block
		let genesis_block_hash = test_data::genesis().hash();
		let inventory = vec![
			InventoryVector {
				inv_type: InventoryType::MessageBlock,
				hash: genesis_block_hash.clone(),
			}
		];
		local_node.on_getdata(peer_index, types::GetData {
			inventory: inventory.clone()
		});
		// => `getdata` is served
		let tasks = server.take_tasks();
		assert_eq!(tasks, vec![ServerTask::GetData(peer_index, types::GetData::with_inventory(inventory))]);
	}

	#[test]
	fn local_node_accepts_local_transaction() {
		let (executor, _, local_node) = create_local_node(None);

		// transaction will be relayed to this peer
		let peer_index1 = 0; local_node.on_connect(peer_index1, "test".into(), types::Version::default());
		executor.take_tasks();

		let genesis = test_data::genesis();
		let transaction: Transaction = test_data::TransactionBuilder::with_output(1).add_input(&genesis.transactions[0], 0).into();
		let transaction_hash = transaction.hash();

		let result = local_node.accept_transaction(transaction.clone().into());
		assert_eq!(result, Ok(transaction_hash.clone()));

		assert_eq!(executor.take_tasks(), vec![Task::RelayNewTransaction(transaction.into(), 83333333)]);
	}

	#[test]
	fn local_node_discards_local_transaction() {
		let genesis = test_data::genesis();
		let transaction: Transaction = test_data::TransactionBuilder::with_output(1).add_input(&genesis.transactions[0], 0).into();
		let transaction_hash = transaction.hash();

		// simulate transaction verification fail
		let mut verifier = DummyVerifier::default();
		verifier.error_when_verifying(transaction_hash.clone(), "simulated");

		let (executor, _, local_node) = create_local_node(Some(verifier));

		let peer_index1 = 0; local_node.on_connect(peer_index1, "test".into(), types::Version::default());
		executor.take_tasks();

		let result = local_node.accept_transaction(transaction.into());
		assert_eq!(result, Err("simulated".to_owned()));

		assert_eq!(executor.take_tasks(), vec![]);
	}
}
