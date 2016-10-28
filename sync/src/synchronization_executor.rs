use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use message::common::{InventoryVector, InventoryType};
use message::types;
use primitives::hash::H256;
use p2p::OutboundSyncConnectionRef;
use synchronization_chain::ChainRef;
use synchronization::{Task as SynchronizationTask, TaskExecutor as SynchronizationTaskExecutor};

pub type LocalSynchronizationTaskExecutorRef = Arc<Mutex<LocalSynchronizationTaskExecutor>>;

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

	pub fn add_peer_connection(&mut self, index: usize, connection: OutboundSyncConnectionRef) {
		self.peers.insert(index, connection);
	}
}

impl SynchronizationTaskExecutor for LocalSynchronizationTaskExecutor {
	fn execute(&mut self, task: SynchronizationTask) {
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
						let connection = &mut *connection;
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
						let connection = &mut *connection;
						trace!(target: "sync", "Querying full inventory from peer#{}", peer_index);
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
						let connection = &mut *connection;
						trace!(target: "sync", "Querying best inventory from peer#{}", peer_index);
						connection.send_getblocks(&getblocks);
					},
					_ => (),
				}
			},
		}
	}
}
