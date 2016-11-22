use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::Mutex;
use chain::{Block, BlockHeader};
use message::common::{InventoryVector, InventoryType};
use message::types;
use primitives::hash::H256;
use p2p::OutboundSyncConnectionRef;
use synchronization_chain::ChainRef;
use synchronization_server::ServerTaskIndex;
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
	/// Request blocks headers using full getheaders.block_locator_hashes.
	RequestBlocksHeaders(usize),
	/// Request memory pool contents
	RequestTransactions(usize, Vec<H256>),
	/// Request memory pool contents
	RequestMemoryPool(usize),
	/// Send block.
	SendBlock(usize, Block, ServerTaskIndex),
	/// Send notfound
	SendNotFound(usize, Vec<InventoryVector>, ServerTaskIndex),
	/// Send inventory
	SendInventory(usize, Vec<InventoryVector>, ServerTaskIndex),
	/// Send headers
	SendHeaders(usize, Vec<BlockHeader>, ServerTaskIndex),
	/// Notify io about ignored request
	Ignore(usize, u32),
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
					trace!(target: "sync", "Querying {} unknown blocks from peer#{}", getdata.inventory.len(), peer_index);
					connection.send_getdata(&getdata);
				}
			},
			Task::RequestBlocksHeaders(peer_index) => {
				let block_locator_hashes = self.chain.read().block_locator_hashes();
				let getheaders = types::GetHeaders {
					version: 0,
					block_locator_hashes: block_locator_hashes,
					hash_stop: H256::default(),
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					trace!(target: "sync", "Request blocks hashes from peer#{} using getheaders", peer_index);
					connection.send_getheaders(&getheaders);
				}
			},
			Task::RequestMemoryPool(peer_index) => {
				let mempool = types::MemPool;

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					trace!(target: "sync", "Querying memory pool contents from peer#{}", peer_index);
					connection.send_mempool(&mempool);
				}
			},
			Task::RequestTransactions(peer_index, transactions_hashes) => {
				let getdata = types::GetData {
					inventory: transactions_hashes.into_iter()
						.map(|hash| InventoryVector {
							inv_type: InventoryType::MessageTx,
							hash: hash,
						}).collect()
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					trace!(target: "sync", "Querying {} unknown transactions from peer#{}", getdata.inventory.len(), peer_index);
					connection.send_getdata(&getdata);
				}
			},
			Task::SendBlock(peer_index, block, id) => {
				let block_message = types::Block {
					block: block,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					assert_eq!(id.raw(), None);
					trace!(target: "sync", "Sending block {:?} to peer#{}", block_message.block.hash(), peer_index);
					connection.send_block(&block_message);
				}
			},
			Task::SendNotFound(peer_index, unknown_inventory, id) => {
				let notfound = types::NotFound {
					inventory: unknown_inventory,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					assert_eq!(id.raw(), None);
					trace!(target: "sync", "Sending notfound to peer#{} with {} items", peer_index, notfound.inventory.len());
					connection.send_notfound(&notfound);
				}
			},
			Task::SendInventory(peer_index, inventory, id) => {
				let inventory = types::Inv {
					inventory: inventory,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					assert_eq!(id.raw(), None);
					trace!(target: "sync", "Sending inventory to peer#{} with {} items", peer_index, inventory.inventory.len());
					connection.send_inventory(&inventory);
				}
			},
			Task::SendHeaders(peer_index, headers, id) => {
				let headers = types::Headers {
					headers: headers,
				};

				if let Some(connection) = self.peers.get_mut(&peer_index) {
					trace!(target: "sync", "Sending headers to peer#{} with {} items", peer_index, headers.headers.len());
					match id.raw() {
						Some(id) => connection.respond_headers(&headers, id),
						None => connection.send_headers(&headers),
					}
				}
			},
			Task::Ignore(peer_index, id) => {
				if let Some(connection) = self.peers.get_mut(&peer_index) {
					trace!(target: "sync", "Ignoring request from peer#{} with id {}", peer_index, id);
					connection.ignored(id);
				}
			},
		}
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use std::sync::Arc;
	use std::mem::replace;
	use std::time;
	use parking_lot::{Mutex, Condvar};
	use local_node::PeersConnections;
	use p2p::OutboundSyncConnectionRef;

	pub struct DummyTaskExecutor {
		tasks: Vec<Task>,
		waiter: Arc<Condvar>,
	}

	impl DummyTaskExecutor {
		pub fn new() -> Arc<Mutex<Self>> {
			Arc::new(Mutex::new(DummyTaskExecutor {
				tasks: Vec::new(),
				waiter: Arc::new(Condvar::new()),
			}))
		}

		pub fn wait_tasks_for(executor: Arc<Mutex<Self>>, timeout_ms: u64) -> Vec<Task> {
			let mut executor = executor.lock();
			if executor.tasks.is_empty() {
				let waiter = executor.waiter.clone();
				waiter.wait_for(&mut executor, time::Duration::from_millis(timeout_ms)).timed_out();
			}
			executor.take_tasks()
		}

		pub fn wait_tasks(executor: Arc<Mutex<Self>>) -> Vec<Task> {
			DummyTaskExecutor::wait_tasks_for(executor, 1000)
		}

		pub fn take_tasks(&mut self) -> Vec<Task> {
			replace(&mut self.tasks, Vec::new())
		}
	}

	impl PeersConnections for DummyTaskExecutor {
		fn add_peer_connection(&mut self, _: usize, _: OutboundSyncConnectionRef) {}
		fn remove_peer_connection(&mut self, _: usize) {}
	}

	impl TaskExecutor for DummyTaskExecutor {
		fn execute(&mut self, task: Task) {
			self.tasks.push(task);
			self.waiter.notify_one();
		}
	}
}
