use std::sync::Arc;
use std::ops::DerefMut;
use std::collections::HashMap;
use parking_lot::Mutex;
use p2p::OutboundSyncConnectionRef;
use primitives::hash::H256;
use message::Payload;
use message::common::InventoryType;
use message::types;
use chain::{Block, Transaction};
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
		let connection = connection.deref_mut();
		peer.version = version;

		// start headers sync
		if peer.version >= types::SendHeaders::version() {
			// send `sendheaders` message to receive `headers` message instead of `inv` message
			trace!(target: "sync", "Sending `sendheaders` to peer#{}", peer_index);
			let sendheaders = types::SendHeaders {};
			connection.send_sendheaders(&sendheaders);
		}

		// send `getheaders` message
		/* TODO: why is it not working with latest bitcoind? (NODE_NETWORK bit is set)
		trace!(target: "sync", "Sending `getheaders` to peer#{}", peer_index);
		let getheaders = types::GetHeaders {
			version: 0,
			block_locator_hashes: self.chain.block_locator_hashes(),
			hash_stop: H256::default(),
		};
		connection.send_getheaders(&getheaders);*/
		trace!(target: "sync", "Sending `getblocks` to peer#{}", peer_index);
		let getblocks = types::GetBlocks {
			version: 0,
			block_locator_hashes: self.chain.block_locator_hashes(),
			hash_stop: H256::default(),
		};
		connection.send_getblocks(&getblocks);
	} 

	pub fn on_peer_inventory(&mut self, peer_index: usize, message: &types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}", peer_index);

		let mut getdata = types::GetData {
			inventory: Vec::new(),
		};
		for item in message.inventory.iter() {
			match InventoryType::from_u32(item.inv_type) {
				Some(InventoryType::MessageBlock) => {
					//trace!(target: "sync", "Peer#{} have block {}", peer_index, item.hash);
					if !self.chain.is_known_block(&item.hash) {
						getdata.inventory.push(item.clone());
					}
				},
				_ => (),
			}
		}

		// request unknown inventory data
		if !getdata.inventory.is_empty() {
			trace!(target: "sync", "Sending `getdata` message to peer#{}", peer_index);
			let peer = self.peers.get_mut(&peer_index).unwrap();
			peer.getdata_requests += getdata.inventory.len();

			let mut connection = peer.connection.lock();
			let connection = connection.deref_mut();
			connection.send_getdata(&getdata);
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

	pub fn on_peer_transaction(&mut self, peer_index: usize, _message: &types::Tx) {
		trace!(target: "sync", "Got `tx` message from peer#{}", peer_index);
	}

	pub fn on_peer_block(&mut self, peer_index: usize, message: &types::Block) {
		trace!(target: "sync", "Got `block` message from peer#{}", peer_index);
		self.chain.insert_block(&message.block);

		let peer = self.peers.get_mut(&peer_index).unwrap();
		peer.getdata_requests -= 1;
		if peer.getdata_requests == 0 {
			trace!(target: "sync", "Sending `getblocks` to peer#{}", peer_index);
			let getblocks = types::GetBlocks {
				version: 0,
				block_locator_hashes: self.chain.block_locator_hashes(),
				hash_stop: H256::default(),
			};

			let mut connection = peer.connection.lock();
			let connection = connection.deref_mut();
			connection.send_getblocks(&getblocks);
		}
	}

	pub fn on_peer_headers(&mut self, peer_index: usize, message: &types::Headers) {
		trace!(target: "sync", "Got `headers` message from peer#{}", peer_index);
		/*let headers_len = message.headers.len();
		if headers_len == 0 {
			return;
		}

		if headers_len == 1 {
			if self.chain.insert_block_header(message.headers[0].clone()).is_err() {
				// TODO: drop connection?
			}
			return;
		}

		// TODO: cannot sort by timestamp (https://en.bitcoin.it/wiki/Block_timestamp)
		// TODO: sort by previous_header_hash instead
		// (almost 100% it's already ordered or reverse ordered => insertion sort)
		let headers_sorted: Vec<_> = if message.headers[1].previous_header_hash == message.headers[0].hash()
			&& self.chain.is_known_block(&message.headers[0].previous_header_hash) {
			message.headers.iter().cloned().collect()
		}
		else if message.headers[0].previous_header_hash == message.headers[1].hash()
			&& self.chain.is_known_block(&message.headers[headers_len - 1].previous_header_hash) {
			message.headers.iter().rev().cloned().collect()
		}
		else {
			// TODO: use sorted headers
			return;
		};

		let best_block_header = headers_sorted[headers_len - 1].clone();
		for block_header in headers_sorted {
			trace!(target: "sync", "Peer#{} have block {}", peer_index, block_header.hash());
			if self.chain.insert_block_header(block_header).is_err() {
				// TODO: drop connection?
				return;
			}
		}

		// query next blocks headers chunk
		let getheaders = types::GetHeaders {
			version: 0,
			block_locator_hashes: vec![best_block_header.hash()],
			hash_stop: H256::default(),
		};
		let ref peer = self.peers[&peer_index];
		peer.connection.lock().send_getheaders(&getheaders);*/
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
