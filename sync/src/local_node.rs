use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use p2p::OutboundSyncConnectionRef;
use primitives::hash::H256;
use message::common::{InventoryVector, InventoryType};
use message::types;
use synchronization::{Synchronization, Task as SynchronizationTask};
use local_chain::LocalChain;
use best_block::BestBlock;

pub type LocalNodeRef = Arc<Mutex<LocalNode>>;

pub struct LocalNode {
	chain: LocalChain,
	peer_counter: usize,
	peers: HashMap<usize, RemoteNode>,
	synchronization: Synchronization,
}

struct RemoteNode {
	connection: OutboundSyncConnectionRef,
}

impl LocalNode {
	pub fn new() -> LocalNodeRef {
		Arc::new(Mutex::new(LocalNode {
			chain: LocalChain::new(),
			peer_counter: 0,
			peers: HashMap::new(),
			synchronization: Synchronization::new(),
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
			connection: outbound_connection,
		});
		self.peer_counter
	}

	pub fn start_sync_session(&mut self, peer_index: usize, _version: u32) {
		trace!(target: "sync", "Starting new sync session with peer#{}", peer_index);

		// request inventory from peer
		self.execute_synchronization_task(SynchronizationTask::RequestInventory(peer_index));
	} 

	pub fn on_peer_inventory(&mut self, peer_index: usize, message: types::Inv) {
		trace!(target: "sync", "Got `inventory` message from peer#{}. Inventory len: {}", peer_index, message.inventory.len());

		// TODO: after each `getblocks` message bitcoind responds with two `inventory` messages:
		// (1) with single entry
		// (2) with 500 entries
		// what is (1)?

		// process unknown blocks
		let unknown_blocks: Vec<_> = message.inventory.iter()
			.filter(|item| InventoryType::from_u32(item.inv_type) == Some(InventoryType::MessageBlock))
			.filter(|item| !self.chain.is_known_block(&item.hash))
			.map(|item| item.hash.clone())
			.collect();

		// if there are unknown blocks => start synchronizing with peer
		if !unknown_blocks.is_empty() {
			self.synchronization.on_unknown_blocks(peer_index, unknown_blocks);
			self.execute_synchronization_tasks();
		}

		// TODO: process unknown transactions, etc...
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
		trace!(target: "sync", "Got `block` message from peer#{}", peer_index);

		// try to process new block
		self.synchronization.on_peer_block(&mut self.chain, peer_index, message.block);
		self.execute_synchronization_tasks();
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

	fn execute_synchronization_tasks(&mut self) {
		for task in self.synchronization.get_synchronization_tasks() {
			self.execute_synchronization_task(task)
		}
	}

	fn execute_synchronization_task(&mut self, task: SynchronizationTask) {
		match task {
			SynchronizationTask::RequestBlocks(peer_index, blocks_hashes) => {
				let getdata = types::GetData {
					inventory: blocks_hashes.into_iter()
						.map(|hash| InventoryVector {
							inv_type: InventoryType::MessageBlock.into(),
							hash: hash,
						}).collect()
				};

				let peer = self.peers.get_mut(&peer_index).unwrap();
				let mut connection = peer.connection.lock();
				let connection = &mut *connection;

				trace!(target: "sync", "Querying {} unknown blocks from peer#{}", getdata.inventory.len(), peer_index);
				connection.send_getdata(&getdata);
			}
			SynchronizationTask::RequestInventory(peer_index) => {
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: self.synchronization.block_locator_hashes(&self.chain),
					hash_stop: H256::default(),
				};

				let peer = self.peers.get_mut(&peer_index).unwrap();
				let mut connection = peer.connection.lock();
				let connection = &mut *connection;

				trace!(target: "sync", "Querying full inventory from peer#{}", peer_index);
				trace!(target: "sync", "Synchronization state: sync = {:?}, chain = {:?}", self.synchronization.information(), self.chain.information());
				connection.send_getblocks(&getblocks);
			},
			SynchronizationTask::RequestBestInventory(peer_index) => {
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: self.synchronization.best_block_locator_hashes(&self.chain),
					hash_stop: H256::default(),
				};

				let peer = self.peers.get_mut(&peer_index).unwrap();
				let mut connection = peer.connection.lock();
				let connection = &mut *connection;

				trace!(target: "sync", "Querying best inventory from peer#{}", peer_index);
				trace!(target: "sync", "Synchronization state: sync = {:?}, chain = {:?}", self.synchronization.information(), self.chain.information());
				connection.send_getblocks(&getblocks);
			},
		}
	}
}
