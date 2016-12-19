use chain::{IndexedBlock, IndexedTransaction};
use message::types;
use network::Magic;
use synchronization_client::Client;
use synchronization_executor::{Executor, Task};
use synchronization_peers::{Peers, BlockAnnouncementType, TransactionAnnouncementType};
use synchronization_server::{Server, ServerTask};
use types::{LOCAL_PEER_INDEX, PeerIndex, RequestId, PeersRef, ClientRef, ExecutorRef, MemoryPoolRef, StorageRef};
use utils::Promise;

// TODO: both Local + Network node impl
// TODO: call getblocktemplate directly from RPC + lock MemoryPool only for reading transactions && then release lock ASAP

/// Local synchronization client node
pub trait LocalClientNode {
	/// When new block comes from local client
	fn push_block(&self, block: IndexedBlock) -> Promise;
	/// When new transaction comes from local client
	fn push_transaction(&self, transaction: IndexedTransaction) -> Promise;
}

/// Network synchronization node
pub trait NetworkNode {
	/// Called when connection is opened
	fn on_connect(&self, peer_index: PeerIndex, version: types::Version);
	/// Called when connection is closed
	fn on_disconnect(&self, peer_index: PeerIndex);
}

/// Network synchronization client node
pub trait NetworkClientNode {
	/// Called upon receiving inventory message
	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv);
	/// Called upon receiving headers message
	fn on_headers(&self, peer_index: PeerIndex, message: types::Headers);
	/// Called upon receiving notfound message
	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound);
	/// Called upon receiving block message
	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock);
	/// Called upon receiving transaction message
	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction);
	/// Called upon receiving merkleblock message
	fn on_merkleblock(&self, peer_index: PeerIndex, message: types::MerkleBlock);
	/// Called upon receiving cmpctblock message
	fn on_cmpctblock(&self, peer_index: PeerIndex, message: types::CompactBlock);
	/// Called upon receiving blocktxn message
	fn on_blocktxn(&self, peer_index: PeerIndex, message: types::BlockTxn);
}

/// Network synchronization server node
pub trait NetworkServerNode: NetworkFiltersNode + NetworkOptionsNode {
	/// Called upon receiving getdata message
	fn on_getdata(&self, peer_index: PeerIndex, message: types::GetData);
	/// Called upon receiving getblocks message
	fn on_getblocks(&self, peer_index: PeerIndex, message: types::GetBlocks);
	/// Called upon receiving getheaders message
	fn on_getheaders(&self, peer_index: PeerIndex, message: types::GetHeaders, request_id: RequestId);
	/// Called upon receiving mempool message
	fn on_mempool(&self, peer_index: PeerIndex, message: types::MemPool);
	/// Called upon receiving getblocktxn message
	fn on_getblocktxn(&self, peer_index: PeerIndex, message: types::GetBlockTxn);
}

/// Network synchronization node with filters support
pub trait NetworkFiltersNode {
	/// Called upon receiving filterload message
	fn on_filterload(&self, peer_index: PeerIndex, message: types::FilterLoad);
	/// Called upon receiving filteradd message
	fn on_filteradd(&self, peer_index: PeerIndex, message: types::FilterAdd);
	/// Called upon receiving filterclear message
	fn on_filterclear(&self, peer_index: PeerIndex, message: types::FilterClear);
	/// Called upon receiving feefilter message
	fn on_feefilter(&self, peer_index: PeerIndex, message: types::FeeFilter);
}

/// Network synchronization node with options support
pub trait NetworkOptionsNode {
	/// Called upon receiving sendheaders message
	fn on_sendheaders(&self, peer_index: PeerIndex, message: types::SendHeaders);
	/// Called upon receiving sendcmpct message
	fn on_sendcmpct(&self, peer_index: PeerIndex, message: types::SendHeaders);
}

/// Network synchronization node implementation
struct NetworkNodeImpl<TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server> {
	/// Network we are currenly operating on
	network: Magic,
	/// Peers reference
	peers: PeersRef<TPeers>,
	/// Executor reference
	executor: ExecutorRef<TExecutor>,
	/// Synchronization client reference
	client: ClientRef<TClient>,
	/// Synchronization server
	server: TServer,
}

/// Local synchronization node implementation
struct LocalNodeImpl<TClient: Client> {
	/// Storage reference
	storage: StorageRef,
	/// Memory pool reference
	memory_pool: MemoryPoolRef,
	/// Synchronization client reference
	client: ClientRef<TClient>,
}

impl<TPeers, TExecutor, TClient, TServer> NetworkNode for NetworkNodeImpl
	where TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server {
	fn on_connect(&self, peer_index: PeerIndex, version: types::Version) {
		trace!(target: "sync", "Starting new sync session with peer#{}", peer_index);

		// light clients may not want transactions broadcasting until filter for connection is set
		if !version.relay_transactions() {
			self.peers.set_transaction_announcement_type(TransactionAnnouncementType::DoNotAnnounce);
		}

		// ask peer for its block headers to find our best common block
		// and possibly start sync session
		self.executor.execute(Task::RequestBlocksHeaders(peer_index));
	}

	fn on_disconnect(&self, peer_index: PeerIndex) {
		trace!(target: "sync", "Stopping sync session with peer#{}", peer_index);

		// forget this connection
		self.peers.remove(peer_index);
		// tell client that this peer has disconneced to adjust synchronization process
		self.client.on_disconnect(peer_index);
		// tell server that this peer has disconneced to stop serving its requests
		self.server.on_disconnect(peer_index);
	}
}

impl<TPeers, TExecutor, TClient, TServer> NetworkClientNode for NetworkNodeImpl
	where TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server {
	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv) {
		trace!(target: "sync", "'inventory' message from peer#{}: {}", peer_index, message.short_info());
		self.client.on_inventory(peer_index, message);
	}

	fn on_headers(&self, peer_index: PeerIndex, message: types::Headers) {
		trace!(target: "sync", "'headers' message from peer#{}: {}", peer_index, message.short_info());
		self.client.on_headers(peer_index, message);
	}

	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound) {
		trace!(target: "sync", "'notfound' message from peer#{}: {}", peer_index, message.short_info());
		self.client.on_notfound(peer_index, message);
	}

	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock) -> Promise {
		trace!(target: "sync", "'block' message from peer#{}: {}", peer_index, block.short_info());
		self.client.on_block(peer_index, block);
	}

	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction) {
		trace!(target: "sync", "'tx' message from peer#{}: {}", peer_index, transaction.short_info());
		self.client.on_transaction(peer_index, transaction);
	}

	fn on_merkleblock(&self, peer_index: PeerIndex, message: types::MerkleBlock) {
		trace!(target: "sync", "'merkleblock' message from peer#{}: {}", peer_index, message.short_info());
		// we never setup filter on connections => misbehaving
		self.peers.misbehaving(peer_index, format!("Got unrequested 'merkleblock' message: {}", message.long_info()));
	}

	fn on_cmpctblock(&self, peer_index: PeerIndex, message: types::CompactBlock) {
		trace!(target: "sync", "'cmpctblock' message from peer#{}: {}", peer_index, message.short_info());
		// we never ask for compact blocks && transactions => misbehaving
		self.peers.misbehaving(peer_index, format!("Got unrequested 'cmpctblock' message: {}", message.long_info()));
	}

	fn on_blocktxn(&self, peer_index: PeerIndex, message: types::BlockTxn) {
		trace!(target: "sync", "'blocktxn' message from peer#{}: {}", peer_index, message.short_info());
		// we never ask for compact blocks && transactions => misbehaving
		self.peers.misbehaving(peer_index, format!("Got unrequested 'blocktxn' message: {}", message.long_info()));
	}
}

impl<TPeers, TExecutor, TClient, TServer> NetworkServerNode for NetworkNodeImpl
	where TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server {
	fn on_getdata(&self, peer_index: PeerIndex, message: types::GetData) {
		// we do not serve any server requests during synchronization
		if self.client.is_synchronizing() {
			trace!(target: "sync", "'getdata' message from peer#{} ignored as sync is in progress: {}", peer_index, message.short_info());
			return;
		}

		trace!(target: "sync", "'getdata' message from peer#{}: {}", peer_index, message.short_info());
		self.server.execute(peer_index, ServerTask::ServeGetData(message));
	}

	fn on_getblocks(&self, peer_index: PeerIndex, message: types::GetBlocks) {
		// we do not serve any server requests during synchronization
		if self.client.is_synchronizing() {
			trace!(target: "sync", "'getblocks' message from peer#{} ignored as sync is in progress: {}", peer_index, message.short_info());
			return;
		}

		trace!(target: "sync", "'getblocks' message from peer#{}: {}", peer_index, message.short_info());
		self.server.execute(peer_index, ServerTask::ServeGetBlocks(message));
	}

	fn on_getheaders(&self, peer_index: PeerIndex, message: types::GetHeaders, request_id: RequestId) {
		// we do not serve any server requests during synchronization
		if self.client.is_synchronizing() {
			trace!(target: "sync", "'getheaders' message from peer#{} ignored as sync is in progress: {}", peer_index, message.short_info());
			self.executor.execute(Task::Ignore(RequestId));
			return;
		}

		trace!(target: "sync", "'getheaders' message from peer#{}: {}", peer_index, message.short_info());
		// during regtests we should simulate bitcoind and respond to incoming requests in-order and 'synchronously':
		// -> block, -> getheaders, -> ping, [wait for verification completed], <- headers, <- pong
		let promise = match self.network {
			Magic::Regtest => self.peer_promises.get(peer_index),
			_ => Promise::completed(),
		};

		// so we are responding with headers only when block verification promise is completed
		// requests are ordered in p2p module, so that no other requests are served until we respond or ignore request_id 
		promise.and_then(|| self.server.execute(peer_index, ServerTask::ServeGetHeaders(message, request_id)));
	}

	fn on_mempool(&self, peer_index: PeerIndex, _message: types::MemPool) {
		// we do not serve any server requests during synchronization
		if self.client.is_synchronizing() {
			trace!(target: "sync", "'mempool' message from peer#{} ignored as sync is in progress", peer_index);
			return;
		}

		trace!(target: "sync", "'mempool' message from peer#{}", peer_index);
		self.server.execute(peer_index, ServerTask::ServeMempool);
	}

	fn on_getblocktxn(&self, peer_index: PeerIndex, message: types::GetBlockTxn) {
		// we do not serve any server requests during synchronization
		if self.client.is_synchronizing() {
			trace!(target: "sync", "'getblocktxn' message from peer#{} ignored as sync is in progress: {}", peer_index, message.short_info());
			return;
		}

		trace!(target: "sync", "'getblocktxn' message from peer#{}: {}", peer_index, message.short_info());
		self.server.execute(peer_index, ServerTask::ServeGetBlockTxn(message));
	}
}

impl<TPeers, TExecutor, TClient, TServer> NetworkFiltersNode for NetworkNodeImpl
	where TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server {
	fn on_filterload(&self, peer_index: PeerIndex, message: types::FilterLoad) {
		trace!(target: "sync", "'filterload' message from peer#{}: {}", peer_index, message.short_info());
		self.peers.set_bloom_filter(peer_index, message);
	}

	fn on_filteradd(&self, peer_index: PeerIndex, message: types::FilterAdd) {
		trace!(target: "sync", "'filteradd' message from peer#{}: {}", peer_index, message.short_info());
		self.peers.update_bloom_filter(peer_index, message);
	}

	fn on_filterclear(&self, peer_index: PeerIndex, message: types::FilterClear) {
		trace!(target: "sync", "'filterclear' message from peer#{}", peer_index);
		self.peers.clear_bloom_filter(peer_index, message);
	}

	fn on_feefilter(&self, peer_index: PeerIndex, message: types::FeeFilter) {
		trace!(target: "sync", "'feefilter' message from peer#{}: {}", peer_index, message.short_info());
		self.peers.set_fee_filter(peer_index, message);
	}
}

impl<TPeers, TExecutor, TClient, TServer> NetworkOptionsNode for NetworkNodeImpl
	where TPeers: Peers, TExecutor: Executor, TClient: Client, TServer: Server {
	fn on_sendheaders(&self, peer_index: PeerIndex, _message: types::SendHeaders) {
		trace!(target: "sync", "'sendheaders' message from peer#{}", peer_index);
		self.peers.set_block_announcement_type(peer_index, BlockAnnouncementType::SendHeaders);
	}

	fn on_sendcmpct(&self, peer_index: PeerIndex, message: types::SendHeaders) {
		trace!(target: "sync", "'sendcmpct' message from peer#{}", peer_index);
		self.peers.set_block_announcement_type(peer_index, BlockAnnouncementType::SendCompact);
	}
}

impl<TClient> LocalClientNode for LocalNodeImpl where TClient: Client {
	fn push_block(&self, block: IndexedBlock) -> Promise {
		trace!(target: "sync", "'block' is received from local client: {}", block.short_info());
		self.client.on_block(LOCAL_PEER_INDEX, block)
	}

	fn push_transaction(&self, transaction: IndexedTransaction) -> Promise {
		trace!(target: "sync", "'transaction' is received from local client: {}", transaction.short_info());
		self.client.on_transaction(LOCAL_PEER_INDEX, transaction)
	}
}
