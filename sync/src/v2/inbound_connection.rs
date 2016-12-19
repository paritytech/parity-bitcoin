use message::types;
use p2p::{InboundSyncConnection, InboundSyncConnectionRef};
use types::{PeersRef, PeerIndex, RequestId};

/// Inbound synchronization connection
#[derive(Debug)]
pub struct InboundConnection {
	/// Index of peer for this connection
	peer_index: PeerIndex,
	/// Peers reference
	peers: PeersRef,
	/// Reference to synchronization node
	node: LocalNodeRef,
}

impl InboundConnection {
	/// Create new inbound connection
	pub fn new(peer_index: PeerIndex, peers: PeersRef, node: LocalNodeRef) -> InboundConnection {
		InboundConnection {
			peer_index: peer_index,
			peers:peers,
			node: node,
		}
	}

	/// Box inbound connection
	pub fn boxed(self) -> InboundSyncConnectionRef {
		Box::new(self)
	}
}

impl InboundSyncConnection for InboundConnection {
	fn start_sync_session(&self, version: u32) {
		self.node.on_connect(self.peer_index, version);
	}

	fn close_session(&self) {
		self.node.on_disconnect(self.peer_index);
	}

	fn on_inventory(&self, message: types::Inv) {
		// if inventory is empty - just ignore this message
		if message.inventory.is_empty() {
			return;
		}
		// if inventory length is too big => possible DOS
		if message.inventory.len() > types::INV_MAX_INVENTORY_LEN {
			self.peers.dos(self.peer_index, format!("'inv' message contains {} entries", message.inventory.len()));
			return;
		}

		self.node.on_inventory(self.peer_index, message);
	}

	fn on_getdata(&self, message: types::GetData) {
		// if inventory is empty - just ignore this message
		if message.inventory.is_empty() {
			return;
		}
		// if inventory length is too big => possible DOS
		if message.inventory.len() > types::GETDATA_MAX_INVENTORY_LEN {
			self.peers.dos(self.peer_index, format!("'getdata' message contains {} entries", message.inventory.len()));
			return;
		}

		self.node.on_getdata(self.peer_index, message);
	}

	fn on_getblocks(&self, message: types::GetBlocks) {
		self.node.on_getblocks(self.peer_index, message);
	}

	fn on_getheaders(&self, message: types::GetHeaders, id: RequestId) {
		self.node.on_getheaders(self.peer_index, message, id);
	}

	fn on_transaction(&self, message: types::Tx) {
		self.node.on_transaction(self.peer_index, message.transaction.into());
	}

	fn on_block(&self, message: types::Block) {
		self.node.on_block(self.peer_index, message.block.into());
	}

	fn on_headers(&self, message: types::Headers) {
		// if headers are empty - just ignore this message
		if message.headers.is_empty() {
			return;
		}
		// if there are too many headers => possible DOS
		if message.headers.len() > types::HEADERS_MAX_HEADERS_LEN {
			self.peers.dos(self.peer_index, format!("'headers' message contains {} headers", message.headers.len()));
			return;
		}

		self.node.on_headers(self.peer_index, message);
	}

	fn on_mempool(&self, message: types::MemPool) {
		self.node.on_peer_mempool(self.peer_index, message);
	}

	fn on_filterload(&self, message: types::FilterLoad) {
		// if filter is too big => possible DOS
		if message.filter.0.len() > types::FILTERLOAD_MAX_FILTER_LEN {
			self.peers.dos(self.peer_index, format!("'filterload' message contains {}-len filter", message.filter.0.len()));
			return;
		}
		// if too many hash functions => possible DOS
		if message.hash_functions > types::FILTERLOAD_MAX_HASH_FUNCS {
			self.peers.dos(self.peer_index, format!("'filterload' message contains {} hash functions", message.hash_functions));
			return;
		}

		self.node.on_peer_filterload(self.peer_index, message);
	}

	fn on_filteradd(&self, message: types::FilterAdd) {
		// if filter item is too big => possible DOS
		if message.data.0.len() > types::FILTERADD_MAX_DATA_LEN {
			self.peers.dos(self.peer_index, format!("'filteradd' message contains {}-len data item", message.data.0.len()));
			return;
		}

		self.node.on_peer_filteradd(self.peer_index, message);
	}

	fn on_filterclear(&self, message: types::FilterClear) {
		self.node.on_peer_filterclear(self.peer_index, message);
	}

	fn on_merkleblock(&self, message: types::MerkleBlock) {
		self.node.on_peer_merkleblock(self.peer_index, message);
	}

	fn on_sendheaders(&self, message: types::SendHeaders) {
		self.node.on_peer_sendheaders(self.peer_index, message);
	}

	fn on_feefilter(&self, message: types::FeeFilter) {
		self.node.on_peer_feefilter(self.peer_index, message);
	}

	fn on_send_compact(&self, message: types::SendCompact) {
		self.node.on_peer_send_compact(self.peer_index, message);
	}

	fn on_compact_block(&self, message: types::CompactBlock) {
		self.node.on_peer_compact_block(self.peer_index, message);
	}

	fn on_get_block_txn(&self, message: types::GetBlockTxn) {
		self.node.on_peer_get_block_txn(self.peer_index, message);
	}

	fn on_block_txn(&self, message: types::BlockTxn) {
		self.node.on_peer_block_txn(self.peer_index, message);
	}

	fn on_notfound(&self, message: types::NotFound) {
		self.node.on_peer_notfound(self.peer_index, message);
	}
}
