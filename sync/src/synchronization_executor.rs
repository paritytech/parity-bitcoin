use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use chain::{Block, RepresentH256};
use message::common::{InventoryVector, InventoryType};
use message::types;
use primitives::hash::H256;
use p2p::OutboundSyncConnectionRef;
use synchronization_chain::ChainRef;
use local_node::PeersConnections;

pub type LocalSynchronizationTaskExecutorRef = Arc<Mutex<LocalSynchronizationTaskExecutor>>;

/// Synchronization task executor
pub trait TaskExecutor : Send + 'static {
	fn execute(&mut self, task: Task);
}

/// Synchronization task for the peer.
#[derive(Debug, PartialEq)]
pub enum Task {
	/// Request given blocks.
	RequestBlocks(usize, Vec<H256>),
	/// Request full inventory using block_locator_hashes.
	RequestInventory(usize),
	/// Request inventory using best block locator only.
	RequestBestInventory(usize),
	/// Send block.
	SendBlock(usize, Block),
	/// Send notfound
	SendNotFound(usize, Vec<InventoryVector>),
	/// Send inventory
	SendInventory(usize, Vec<InventoryVector>),
}

/// Synchronization tasks executor
pub struct LocalSynchronizationTaskExecutor {
	/// Active synchronization peers
	peers: HashMap<usize, OutboundSyncConnectionRef>,
	/// Synchronization chain
	chain: ChainRef,
}

impl LocalSynchronizationTaskExecutor {
	pub fn new(chain: ChainRef) -> Arc<Mutex<Self>> {
		Arc::new(Mutex::new(LocalSynchronizationTaskExecutor {
			peers: HashMap::new(),
			chain: chain,
		}))
	}
}

impl PeersConnections for LocalSynchronizationTaskExecutor {
	fn add_peer_connection(&mut self, index: usize, connection: OutboundSyncConnectionRef) {
		self.peers.insert(index, connection);
	}

	fn remove_peer_connection(&mut self, index: usize) {
		self.peers.remove(&index);
	}
}

impl TaskExecutor for LocalSynchronizationTaskExecutor {
	fn execute(&mut self, task: Task) {
		// TODO: what is types::GetBlocks::version here? (@ PR#37)

		match task {
			Task::RequestBlocks(peer_index, blocks_hashes) => {
				let getdata = types::GetData {
					inventory: blocks_hashes.into_iter()
						.map(|hash| InventoryVector {
							inv_type: InventoryType::MessageBlock,
							hash: hash,
						}).collect()
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Querying {} unknown blocks from peer#{}", getdata.inventory.len(), peer_index);
					connection.send_getdata(&getdata);
				}
			}
			Task::RequestInventory(peer_index) => {
				let block_locator_hashes = self.chain.read().block_locator_hashes();
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: block_locator_hashes,
					hash_stop: H256::default(),
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Querying full inventory from peer#{}", peer_index);
					connection.send_getblocks(&getblocks);
				}
			},
			Task::RequestBestInventory(peer_index) => {
				let block_locator_hashes = self.chain.read().best_block_locator_hashes();
				let getblocks = types::GetBlocks {
					version: 0,
					block_locator_hashes: block_locator_hashes,
					hash_stop: H256::default(),
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Querying best inventory from peer#{}", peer_index);
					connection.send_getblocks(&getblocks);
				}
			},
			Task::SendBlock(peer_index, block) => {
				let block_message = types::Block {
					block: block,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Sending block {:?} to peer#{}", block_message.block.hash(), peer_index);
					connection.send_block(&block_message);
				}
			},
			Task::SendNotFound(peer_index, unknown_inventory) => {
				let notfound = types::NotFound {
					inventory: unknown_inventory,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Sending notfound to peer#{} with {} items", peer_index, notfound.inventory.len());
					connection.send_notfound(&notfound);
				}
			},
			Task::SendInventory(peer_index, inventory) => {
				let inventory = types::Inv {
					inventory: inventory,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					let connection = &mut *connection;
					trace!(target: "sync", "Sending inventory to peer#{} with {} items", peer_index, inventory.inventory.len());
					connection.send_inventory(&inventory);
				}
			},
		}
	}
}
