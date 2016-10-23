use std::sync::Arc;
use std::ops::DerefMut;
use std::collections::HashMap;
use parking_lot::Mutex;
use p2p::protocol::sync::OutboundSyncConnectionRef;
use primitives::hash::H256;
use message::common::InventoryType;
use message::types;
use chain::{Block, Transaction};
use local_chain::LocalChain;
use best_block::BestBlock;

pub type LocalNodeRef = Arc<Mutex<LocalNode>>;

pub struct LocalNode {
	peer_counter: usize,
	chain: LocalChain,
	connections: HashMap<usize, OutboundSyncConnectionRef>,
}

impl LocalNode {
	pub fn new() -> LocalNodeRef {
		Arc::new(Mutex::new(LocalNode {
			peer_counter: 0,
			chain: LocalChain::new(),
			connections: HashMap::new(),
		}))
	}

	pub fn best_block(&self) -> BestBlock {
		self.chain.best_block()
	}

	pub fn start_sync_session(&mut self, _best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> usize {
		// save connection for future
		self.peer_counter += 1;
		self.connections.insert(self.peer_counter, outbound_connection.clone());
		trace!(target: "sync", "Starting new sync session with peer#{}", self.peer_counter);

		// start headers sync
		{
			let mut outbound_connection = outbound_connection.lock();
			let outbound_connection = outbound_connection.deref_mut();
			// send `sendheaders` message to receive `headers` message instead of `inv` message
			let sendheaders = types::SendHeaders {};
			outbound_connection.send_sendheaders(&sendheaders);
			// send `getheaders` message
			let getheaders = types::GetHeaders {
				version: 0,
				block_locator_hashes: self.chain.block_locator_hashes(),
				hash_stop: H256::default(),
			};
			outbound_connection.send_getheaders(&getheaders);
		}

		self.peer_counter
	} 

	pub fn on_peer_inventory(&mut self, peer_index: usize, message: &types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}", peer_index);
		for item in message.inventory.iter() {
			match InventoryType::from_u32(item.inv_type) {
				Some(InventoryType::MessageBlock) => {
					trace!(target: "sync", "Peer#{} have block {}", peer_index, item.hash);
				},
				_ => (),
			}
		}
	}

	pub fn on_peer_getdata(&mut self, peer_index: usize, _message: &types::GetData) {
		trace!(target: "sync", "Got `getdata` message from peer#{}", peer_index);
	}

	pub fn on_peer_getblocks(&mut self, peer_index: usize, _message: &types::GetBlocks) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);
	}

	pub fn on_peer_getheaders(&mut self, peer_index: usize, _message: &types::GetHeaders) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_transaction(&mut self, peer_index: usize, _message: &Transaction) {
		trace!(target: "sync", "Got `tx` message from peer#{}", peer_index);
	}

	pub fn on_peer_block(&mut self, peer_index: usize, _message: &Block) {
		trace!(target: "sync", "Got `block` message from peer#{}", peer_index);
	}

	pub fn on_peer_headers(&mut self, peer_index: usize, message: &types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}", peer_index);
		for block_header in message.headers.iter() {
			trace!(target: "sync", "Peer#{} have block {}", peer_index, block_header.hash());
		}
	}

	pub fn on_peer_mempool(&mut self, peer_index: usize, _message: &types::MemPool) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterload(&mut self, peer_index: usize, _message: &types::FilterLoad) {
		trace!(target: "sync", "Got `filterload` message from peer#{}", peer_index);
	}

	pub fn on_peer_filteradd(&mut self, peer_index: usize, _message: &types::FilterAdd) {
		trace!(target: "sync", "Got `filteradd` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterclear(&mut self, peer_index: usize, _message: &types::FilterClear) {
		trace!(target: "sync", "Got `filterclear` message from peer#{}", peer_index);
	}

	pub fn on_peer_merkleblock(&mut self, peer_index: usize, _message: &types::MerkleBlock) {
		trace!(target: "sync", "Got `merkleblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_sendheaders(&mut self, peer_index: usize, _message: &types::SendHeaders) {
		trace!(target: "sync", "Got `sendheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_feefilter(&mut self, peer_index: usize, _message: &types::FeeFilter) {
		trace!(target: "sync", "Got `feefilter` message from peer#{}", peer_index);
	}

	pub fn on_peer_send_compact(&mut self, peer_index: usize, _message: &types::SendCompact) {
		trace!(target: "sync", "Got `sendcmpct` message from peer#{}", peer_index);
	}

	pub fn on_peer_compact_block(&mut self, peer_index: usize, _message: &types::CompactBlock) {
		trace!(target: "sync", "Got `cmpctblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_get_block_txn(&mut self, peer_index: usize, _message: &types::GetBlockTxn) {
		trace!(target: "sync", "Got `getblocktxn` message from peer#{}", peer_index);
	}

	pub fn on_peer_block_txn(&mut self, peer_index: usize, _message: &types::BlockTxn) {
		trace!(target: "sync", "Got `blocktxn` message from peer#{}", peer_index);
	}
}
