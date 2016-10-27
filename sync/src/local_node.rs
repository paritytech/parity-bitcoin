use std::sync::Arc;
use std::collections::HashMap;
use db;
use parking_lot::{Mutex, RwLock};
use p2p::OutboundSyncConnectionRef;
use primitives::hash::H256;
use message::common::{InventoryVector, InventoryType};
use message::types;
use synchronization::{Synchronization, Task as SynchronizationTask};
use synchronization_chain::{Chain, ChainRef, BlockState};
use best_block::BestBlock;

/// Thread-safe reference to the `LocalNode`
pub type LocalNodeRef = Arc<Mutex<LocalNode>>;

/// Local synchronization node
pub struct LocalNode {
	/// Throughout counter of synchronization peers
	peer_counter: usize,
	/// Active synchronization peers
	peers: HashMap<usize, OutboundSyncConnectionRef>,
	/// Synchronization chain
	chain: ChainRef,
	/// Synchronization process
	sync: Synchronization,
}

impl LocalNode {
	/// New synchronization node with given storage
	pub fn new(storage: Arc<db::Store>) -> LocalNodeRef {
		let chain = ChainRef::new(RwLock::new(Chain::new(storage)));
		Arc::new(Mutex::new(LocalNode {
			peer_counter: 0,
			peers: HashMap::new(),
			chain: chain.clone(),
			sync: Synchronization::new(chain),
		}))
	}

	/// Best block hash (including non-verified, requested && non-requested blocks)
	pub fn best_block(&self) -> BestBlock {
		self.chain.read().best_block()
	}

	pub fn create_sync_session(&mut self, _best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> usize {
		trace!(target: "sync", "Creating new sync session with peer#{}", self.peer_counter + 1);

		// save connection for future
		self.peer_counter += 1;
		self.peers.insert(self.peer_counter, outbound_connection);
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
		let unknown_blocks: Vec<_> = {
			let chain = self.chain.read();
			message.inventory.iter()
				.filter(|item| InventoryType::from_u32(item.inv_type) == Some(InventoryType::MessageBlock))
				.filter(|item| chain.block_state(&item.hash) == BlockState::Unknown)
				.map(|item| item.hash.clone())
				.collect()
		};

		// if there are unknown blocks => start synchronizing with peer
		if !unknown_blocks.is_empty() {
			self.sync.on_unknown_blocks(peer_index, unknown_blocks);
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
		self.sync.on_peer_block(peer_index, message.block);
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
		for task in self.sync.get_synchronization_tasks() {
			self.execute_synchronization_task(task)
		}
	}

	fn execute_synchronization_task(&mut self, task: SynchronizationTask) {
		// TODO: what is types::GetBlocks::version here? (@ PR#37)

		match task {
			SynchronizationTask::RequestBlocks(peer_index, blocks_hashes) => {
				let getdata = types::GetData {
					inventory: blocks_hashes.into_iter()
						.map(|hash| InventoryVector {
							inv_type: InventoryType::MessageBlock.into(),
							hash: hash,
						}).collect()
				};

				match self.peers.get_mut(&peer_index) {
					Some(connection) => {
						let connection = &mut *connection.lock();
						trace!(target: "sync", "Querying {} unknown blocks from peer#{}", getdata.inventory.len(), peer_index);
						connection.send_getdata(&getdata);
					}
					_ => (),
				}
			}
			SynchronizationTask::RequestInventory(peer_index) => {
				let block_locator_hashes = self.chain.read().block_locator_hashes();
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: block_locator_hashes,
					hash_stop: H256::default(),
				};

				match self.peers.get_mut(&peer_index) {
					Some(connection) => {
						let connection = &mut *connection.lock();
						trace!(target: "sync", "Querying full inventory from peer#{}", peer_index);
						trace!(target: "sync", "Synchronization state: sync = {:?}", self.sync.information());
						connection.send_getblocks(&getblocks);
					},
					_ => (),
				}
			},
			SynchronizationTask::RequestBestInventory(peer_index) => {
				let block_locator_hashes = self.chain.read().best_block_locator_hashes();
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: block_locator_hashes,
					hash_stop: H256::default(),
				};

				match self.peers.get_mut(&peer_index) {
					Some(connection) => {
						let connection = &mut *connection.lock();
						trace!(target: "sync", "Querying best inventory from peer#{}", peer_index);
						trace!(target: "sync", "Synchronization state: {:?}", self.sync.information());
						connection.send_getblocks(&getblocks);
					},
					_ => (),
				}
			},
		}
	}
}
