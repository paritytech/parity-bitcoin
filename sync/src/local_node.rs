use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use p2p::OutboundSyncConnectionRef;
use primitives::hash::H256;
use message::Payload;
use message::common::InventoryType;
use message::types;
use local_chain::LocalChain;
use best_block::BestBlock;

pub type LocalNodeRef = Arc<Mutex<LocalNode>>;

pub struct LocalNode {
	peer_counter: usize,
	chain: LocalChain,
	peers: HashMap<usize, RemoteNode>,
}

struct RemoteNode {
	version: u32,
	connection: OutboundSyncConnectionRef,
	getdata_requests: usize,
}

impl LocalNode {
	pub fn new() -> LocalNodeRef {
		Arc::new(Mutex::new(LocalNode {
			peer_counter: 0,
			chain: LocalChain::new(),
			peers: HashMap::new(),
		}))
	}

	pub fn best_block(&self) -> BestBlock {
		self.chain.best_block()
	}

	pub fn create_sync_session(&mut self, _best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> usize {
		trace!(target: "sync", "Creating new sync session with peer#{}", self.peer_counter + 1);

		// save connection for future
		self.peer_counter += 1;
		self.peers.insert(self.peer_counter, RemoteNode {
			version: 0,
			connection: outbound_connection.clone(),
			getdata_requests: 0,
		});
		self.peer_counter
	}

	pub fn start_sync_session(&mut self, peer_index: usize, version: u32) {
		trace!(target: "sync", "Starting new sync session with peer#{}", peer_index);

		let peer = self.peers.get_mut(&peer_index).unwrap();
		let mut connection = peer.connection.lock();
		let connection = &mut *connection;
		peer.version = version;

		// start headers sync
		if peer.version >= types::SendHeaders::version() {
			// send `sendheaders` message to receive `headers` message instead of `inv` message
			trace!(target: "sync", "Sending `sendheaders` to peer#{}", peer_index);
			let sendheaders = types::SendHeaders {};
			connection.send_sendheaders(&sendheaders);
			// TODO: why we do not support `sendheaders`?
			// TODO: why latest bitcoind doesn't responds to the `getheaders` message?
			// TODO: `getheaders` can be used only after `sendheaders`?
		}

		// get peer' inventory with newest blocks
		trace!(target: "sync", "Sending `getblocks` to peer#{}", peer_index);
		let getblocks = types::GetBlocks {
			version: 0,
			block_locator_hashes: self.chain.block_locator_hashes(),
			hash_stop: H256::default(),
		};
		connection.send_getblocks(&getblocks);
	} 

	pub fn on_peer_inventory(&mut self, peer_index: usize, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());

		// TODO: after each `getblocks` message bitcoind responds with two `inventory` messages:
		// (1) with single entry
		// (2) with 500 entries
		// what if (1)?

		let mut getdata = types::GetData {
			inventory: Vec::new(),
		};
		for item in message.inventory.iter() {
			match InventoryType::from_u32(item.inv_type) {
				Some(InventoryType::MessageBlock) => {
					if !self.chain.is_known_block(&item.hash) {
						getdata.inventory.push(item.clone());
					}
				},
				_ => (),
			}
		}

		// request unknown inventory data
		if !getdata.inventory.is_empty() {
			trace!(target: "sync", "Sending `getdata` message to peer#{}. Querying #{} unknown blocks", peer_index, getdata.inventory.len());
			let peer = self.peers.get_mut(&peer_index).unwrap();
			peer.getdata_requests += getdata.inventory.len();

			let mut connection = peer.connection.lock();
			let connection = &mut *connection;
			connection.send_getdata(&getdata);
		}
	}

	pub fn on_peer_getdata(&mut self, peer_index: usize, _message: types::GetData) {
		trace!(target: "sync", "Got `getdata` message from peer#{}", peer_index);
	}

	pub fn on_peer_getblocks(&mut self, peer_index: usize, _message: types::GetBlocks) {
		trace!(target: "sync", "Got `getblocks` message from peer#{}", peer_index);
	}

	pub fn on_peer_getheaders(&mut self, peer_index: usize, _message: types::GetHeaders) {
		trace!(target: "sync", "Got `getheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_transaction(&mut self, peer_index: usize, _message: types::Tx) {
		trace!(target: "sync", "Got `tx` message from peer#{}", peer_index);
	}

	pub fn on_peer_block(&mut self, peer_index: usize, message: types::Block) {
		// insert block to the chain
		self.chain.insert_block(&message.block);

		// decrease pending requests count
		let peer = self.peers.get_mut(&peer_index).unwrap();
		peer.getdata_requests -= 1;
		trace!(target: "sync", "Got `block` message from peer#{}. Pending #{} requests", peer_index, peer.getdata_requests);

		// if there are no pending requests, continue with next blocks chunk
		if peer.getdata_requests == 0 {
			trace!(target: "sync", "Sending `getblocks` to peer#{}. Local chain state: {:?}", peer_index, self.chain.info());
			let getblocks = types::GetBlocks {
				version: 0,
				block_locator_hashes: self.chain.block_locator_hashes(),
				hash_stop: H256::default(),
			};

			let mut connection = peer.connection.lock();
			let connection = &mut *connection;
			connection.send_getblocks(&getblocks);
		}
	}

	pub fn on_peer_headers(&mut self, peer_index: usize, _message: types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}", peer_index);
	}

	pub fn on_peer_mempool(&mut self, peer_index: usize, _message: types::MemPool) {
		trace!(target: "sync", "Got `mempool` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterload(&mut self, peer_index: usize, _message: types::FilterLoad) {
		trace!(target: "sync", "Got `filterload` message from peer#{}", peer_index);
	}

	pub fn on_peer_filteradd(&mut self, peer_index: usize, _message: types::FilterAdd) {
		trace!(target: "sync", "Got `filteradd` message from peer#{}", peer_index);
	}

	pub fn on_peer_filterclear(&mut self, peer_index: usize, _message: types::FilterClear) {
		trace!(target: "sync", "Got `filterclear` message from peer#{}", peer_index);
	}

	pub fn on_peer_merkleblock(&mut self, peer_index: usize, _message: types::MerkleBlock) {
		trace!(target: "sync", "Got `merkleblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_sendheaders(&mut self, peer_index: usize, _message: types::SendHeaders) {
		trace!(target: "sync", "Got `sendheaders` message from peer#{}", peer_index);
	}

	pub fn on_peer_feefilter(&mut self, peer_index: usize, _message: types::FeeFilter) {
		trace!(target: "sync", "Got `feefilter` message from peer#{}", peer_index);
	}

	pub fn on_peer_send_compact(&mut self, peer_index: usize, _message: types::SendCompact) {
		trace!(target: "sync", "Got `sendcmpct` message from peer#{}", peer_index);
	}

	pub fn on_peer_compact_block(&mut self, peer_index: usize, _message: types::CompactBlock) {
		trace!(target: "sync", "Got `cmpctblock` message from peer#{}", peer_index);
	}

	pub fn on_peer_get_block_txn(&mut self, peer_index: usize, _message: types::GetBlockTxn) {
		trace!(target: "sync", "Got `getblocktxn` message from peer#{}", peer_index);
	}

	pub fn on_peer_block_txn(&mut self, peer_index: usize, _message: types::BlockTxn) {
		trace!(target: "sync", "Got `blocktxn` message from peer#{}", peer_index);
	}
}
