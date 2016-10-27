use parking_lot::Mutex;
use message::types;
use p2p::{InboundSyncConnection, InboundSyncConnectionRef};
use local_node::LocalNodeRef;

// TODO: too many locks (connection is locked -> node is locked -> chain is locked)
// at least connection must be lock-safe

pub struct InboundConnection {
	local_node: LocalNodeRef,
	peer_index: usize,
}

impl InboundConnection {
	pub fn new(local_node: LocalNodeRef, peer_index: usize) -> InboundSyncConnectionRef {
		InboundSyncConnectionRef::new(Mutex::new(Box::new(InboundConnection {
			local_node: local_node,
			peer_index: peer_index,
		})))
	}
}

impl InboundSyncConnection for InboundConnection {
	fn start_sync_session(&mut self, version: u32) {
		self.local_node.lock().start_sync_session(self.peer_index, version);
	}

	fn on_inventory(&mut self, message: types::Inv) {
		self.local_node.lock().on_peer_inventory(self.peer_index, message);
	}

	fn on_getdata(&mut self, message: types::GetData) {
		self.local_node.lock().on_peer_getdata(self.peer_index, message);
	}

	fn on_getblocks(&mut self, message: types::GetBlocks) {
		self.local_node.lock().on_peer_getblocks(self.peer_index, message);
	}

	fn on_getheaders(&mut self, message: types::GetHeaders) {
		self.local_node.lock().on_peer_getheaders(self.peer_index, message);
	}

	fn on_transaction(&mut self, message: types::Tx) {
		self.local_node.lock().on_peer_transaction(self.peer_index, message);
	}

	fn on_block(&mut self, message: types::Block) {
		self.local_node.lock().on_peer_block(self.peer_index, message);
	}

	fn on_headers(&mut self, message: types::Headers) {
		self.local_node.lock().on_peer_headers(self.peer_index, message);
	}

	fn on_mempool(&mut self, message: types::MemPool) {
		self.local_node.lock().on_peer_mempool(self.peer_index, message);
	}

	fn on_filterload(&mut self, message: types::FilterLoad) {
		self.local_node.lock().on_peer_filterload(self.peer_index, message);
	}

	fn on_filteradd(&mut self, message: types::FilterAdd) {
		self.local_node.lock().on_peer_filteradd(self.peer_index, message);
	}

	fn on_filterclear(&mut self, message: types::FilterClear) {
		self.local_node.lock().on_peer_filterclear(self.peer_index, message);
	}

	fn on_merkleblock(&mut self, message: types::MerkleBlock) {
		self.local_node.lock().on_peer_merkleblock(self.peer_index, message);
	}

	fn on_sendheaders(&mut self, message: types::SendHeaders) {
		self.local_node.lock().on_peer_sendheaders(self.peer_index, message);
	}

	fn on_feefilter(&mut self, message: types::FeeFilter) {
		self.local_node.lock().on_peer_feefilter(self.peer_index, message);
	}

	fn on_send_compact(&mut self, message: types::SendCompact) {
		self.local_node.lock().on_peer_send_compact(self.peer_index, message);
	}

	fn on_compact_block(&mut self, message: types::CompactBlock) {
		self.local_node.lock().on_peer_compact_block(self.peer_index, message);
	}

	fn on_get_block_txn(&mut self, message: types::GetBlockTxn) {
		self.local_node.lock().on_peer_get_block_txn(self.peer_index, message);
	}

	fn on_block_txn(&mut self, message: types::BlockTxn) {
		self.local_node.lock().on_peer_block_txn(self.peer_index, message);
	}
}
