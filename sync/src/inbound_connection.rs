use chain::{IndexedTransaction, IndexedBlock, IndexedBlockHeader};
use message::types;
use p2p::{InboundSyncConnection, InboundSyncConnectionRef, InboundSyncConnectionStateRef};
use types::{PeersRef, LocalNodeRef, PeerIndex, RequestId};
use utils::KnownHashType;

/// Inbound synchronization connection
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
			peers: peers,
			node: node,
		}
	}

	/// Box inbound connection
	pub fn boxed(self) -> InboundSyncConnectionRef {
		Box::new(self)
	}
}

impl InboundSyncConnection for InboundConnection {
	fn sync_state(&self) -> InboundSyncConnectionStateRef {
		self.node.sync_state()
	}

	fn start_sync_session(&self, peer_name: String, version: types::Version) {
		self.node.on_connect(self.peer_index, peer_name, version);
	}

	fn close_session(&self) {
		self.peers.remove(self.peer_index);
		self.node.on_disconnect(self.peer_index);
	}

	fn on_inventory(&self, message: types::Inv) {
		// if inventory is empty - just ignore this message
		if message.inventory.is_empty() {
			return;
		}
		// if inventory length is too big => possible DOS
		if message.inventory.len() > types::INV_MAX_INVENTORY_LEN {
			self.peers.dos(self.peer_index, &format!("'inv' message contains {} entries", message.inventory.len()));
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
			self.peers.dos(self.peer_index, &format!("'getdata' message contains {} entries", message.inventory.len()));
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
		let tx = IndexedTransaction::from_raw(message.transaction);
		self.peers.hash_known_as(self.peer_index, tx.hash.clone(), KnownHashType::Transaction);
		self.node.on_transaction(self.peer_index, tx);
	}

	fn on_block(&self, message: types::Block) {
		let block = IndexedBlock::from_raw(message.block);
		self.peers.hash_known_as(self.peer_index, block.hash().clone(), KnownHashType::Block);
		self.node.on_block(self.peer_index, block);
	}

	fn on_headers(&self, message: types::Headers) {
		// if headers are empty - just ignore this message
		if message.headers.is_empty() {
			return;
		}
		// if there are too many headers => possible DOS
		if message.headers.len() > types::HEADERS_MAX_HEADERS_LEN {
			self.peers.dos(self.peer_index, &format!("'headers' message contains {} headers", message.headers.len()));
			return;
		}

		let headers = message.headers.into_iter().map(IndexedBlockHeader::from_raw).collect();
		self.node.on_headers(self.peer_index, headers);
	}

	fn on_mempool(&self, message: types::MemPool) {
		self.node.on_mempool(self.peer_index, message);
	}

	fn on_filterload(&self, message: types::FilterLoad) {
		// if filter is too big => possible DOS
		if message.filter.len() > types::FILTERLOAD_MAX_FILTER_LEN {
			self.peers.dos(self.peer_index, &format!("'filterload' message contains {}-len filter", message.filter.len()));
			return;
		}
		// if too many hash functions => possible DOS
		if message.hash_functions as usize > types::FILTERLOAD_MAX_HASH_FUNCS {
			self.peers.dos(self.peer_index, &format!("'filterload' message contains {} hash functions", message.hash_functions));
			return;
		}

		self.node.on_filterload(self.peer_index, message);
	}

	fn on_filteradd(&self, message: types::FilterAdd) {
		// if filter item is too big => possible DOS
		if message.data.len() > types::FILTERADD_MAX_DATA_LEN {
			self.peers.dos(self.peer_index, &format!("'filteradd' message contains {}-len data item", message.data.len()));
			return;
		}

		self.node.on_filteradd(self.peer_index, message);
	}

	fn on_filterclear(&self, message: types::FilterClear) {
		self.node.on_filterclear(self.peer_index, message);
	}

	fn on_merkleblock(&self, message: types::MerkleBlock) {
		self.node.on_merkleblock(self.peer_index, message);
	}

	fn on_sendheaders(&self, message: types::SendHeaders) {
		self.node.on_sendheaders(self.peer_index, message);
	}

	fn on_feefilter(&self, message: types::FeeFilter) {
		self.node.on_feefilter(self.peer_index, message);
	}

	fn on_send_compact(&self, message: types::SendCompact) {
		self.node.on_send_compact(self.peer_index, message);
	}

	fn on_compact_block(&self, message: types::CompactBlock) {
		self.node.on_compact_block(self.peer_index, message);
	}

	fn on_get_block_txn(&self, message: types::GetBlockTxn) {
		self.node.on_get_block_txn(self.peer_index, message);
	}

	fn on_block_txn(&self, message: types::BlockTxn) {
		self.node.on_block_txn(self.peer_index, message);
	}

	fn on_notfound(&self, message: types::NotFound) {
		self.node.on_notfound(self.peer_index, message);
	}
}

#[cfg(test)]
pub mod tests {
	use std::collections::HashMap;
	use std::sync::Arc;
	use parking_lot::Mutex;
	use message::types;
	use p2p::OutboundSyncConnection;
	use types::RequestId;

	pub struct DummyOutboundSyncConnection {
		pub messages: Mutex<HashMap<String, usize>>,
	}

	impl DummyOutboundSyncConnection {
		pub fn new() -> Arc<DummyOutboundSyncConnection> {
			Arc::new(DummyOutboundSyncConnection {
				messages: Mutex::new(HashMap::new()),
			})
		}
	}

	impl OutboundSyncConnection for DummyOutboundSyncConnection {
		fn send_inventory(&self, _message: &types::Inv) { *self.messages.lock().entry("inventory".to_owned()).or_insert(0) += 1; }
		fn send_getdata(&self, _message: &types::GetData) { *self.messages.lock().entry("getdata".to_owned()).or_insert(0) += 1; }
		fn send_getblocks(&self, _message: &types::GetBlocks) { *self.messages.lock().entry("getblocks".to_owned()).or_insert(0) += 1; }
		fn send_getheaders(&self, _message: &types::GetHeaders) { *self.messages.lock().entry("getheaders".to_owned()).or_insert(0) += 1; }
		fn send_transaction(&self, _message: &types::Tx) { *self.messages.lock().entry("transaction".to_owned()).or_insert(0) += 1; }
		fn send_block(&self, _message: &types::Block) { *self.messages.lock().entry("block".to_owned()).or_insert(0) += 1; }
		fn send_witness_transaction(&self, _message: &types::Tx) { *self.messages.lock().entry("witness_transaction".to_owned()).or_insert(0) += 1; }
		fn send_witness_block(&self, _message: &types::Block) { *self.messages.lock().entry("witness_block".to_owned()).or_insert(0) += 1; }
		fn send_headers(&self, _message: &types::Headers) { *self.messages.lock().entry("headers".to_owned()).or_insert(0) += 1; }
		fn respond_headers(&self, _message: &types::Headers, _id: RequestId) { *self.messages.lock().entry("headers".to_owned()).or_insert(0) += 1; }
		fn send_mempool(&self, _message: &types::MemPool) { *self.messages.lock().entry("mempool".to_owned()).or_insert(0) += 1; }
		fn send_filterload(&self, _message: &types::FilterLoad) { *self.messages.lock().entry("filterload".to_owned()).or_insert(0) += 1; }
		fn send_filteradd(&self, _message: &types::FilterAdd) { *self.messages.lock().entry("filteradd".to_owned()).or_insert(0) += 1; }
		fn send_filterclear(&self, _message: &types::FilterClear) { *self.messages.lock().entry("filterclear".to_owned()).or_insert(0) += 1; }
		fn send_merkleblock(&self, _message: &types::MerkleBlock) { *self.messages.lock().entry("merkleblock".to_owned()).or_insert(0) += 1; }
		fn send_sendheaders(&self, _message: &types::SendHeaders) { *self.messages.lock().entry("sendheaders".to_owned()).or_insert(0) += 1; }
		fn send_feefilter(&self, _message: &types::FeeFilter) { *self.messages.lock().entry("feefilter".to_owned()).or_insert(0) += 1; }
		fn send_send_compact(&self, _message: &types::SendCompact) { *self.messages.lock().entry("sendcompact".to_owned()).or_insert(0) += 1; }
		fn send_compact_block(&self, _message: &types::CompactBlock) { *self.messages.lock().entry("cmpctblock".to_owned()).or_insert(0) += 1; }
		fn send_get_block_txn(&self, _message: &types::GetBlockTxn) { *self.messages.lock().entry("getblocktxn".to_owned()).or_insert(0) += 1; }
		fn send_block_txn(&self, _message: &types::BlockTxn) { *self.messages.lock().entry("blocktxn".to_owned()).or_insert(0) += 1; }
		fn send_notfound(&self, _message: &types::NotFound) { *self.messages.lock().entry("notfound".to_owned()).or_insert(0) += 1; }
		fn ignored(&self, _id: RequestId) {}
		fn close(&self) {}
	}
}
