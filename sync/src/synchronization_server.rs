use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::collections::{VecDeque, HashMap};
use std::collections::hash_map::Entry;
use parking_lot::{Mutex, Condvar};
use message::common::{InventoryVector, InventoryType};
use db;
use primitives::hash::H256;
use synchronization_chain::ChainRef;
use synchronization_executor::{Task, TaskExecutor};
use message::types;

/// Synchronization requests server trait
pub trait Server : Send + 'static {
	fn serve_getdata(&mut self, peer_index: usize, message: types::GetData);
	fn serve_getblocks(&mut self, peer_index: usize, message: types::GetBlocks);
}

/// Synchronization requests server
pub struct SynchronizationServer {
	chain: ChainRef,
	queue_ready: Arc<Condvar>,
	queue: Arc<Mutex<ServerQueue>>,
	worker_thread: Option<thread::JoinHandle<()>>,
}

struct ServerQueue {
	is_stopping: AtomicBool,
	queue_ready: Arc<Condvar>,
	peers_queue: VecDeque<usize>,
	tasks_queue: HashMap<usize, VecDeque<ServerTask>>,
}

#[derive(Debug, PartialEq)]
pub enum ServerTask {
	ServeGetData(Vec<InventoryVector>),
	ServeGetBlocks(db::BestBlock, H256),
	ReturnNotFound(Vec<InventoryVector>),
	ReturnBlock(H256),
}

impl SynchronizationServer {
	pub fn new<T: TaskExecutor>(chain: ChainRef, executor: Arc<Mutex<T>>) -> Self {
		let queue_ready = Arc::new(Condvar::new());
		let queue = Arc::new(Mutex::new(ServerQueue::new(queue_ready.clone())));
		let mut server = SynchronizationServer {
			chain: chain.clone(),
			queue_ready: queue_ready.clone(),
			queue: queue.clone(),
			worker_thread: None,
		};
		server.worker_thread = Some(thread::spawn(move || {
			SynchronizationServer::server_worker(queue_ready, queue, chain, executor);
		}));
		server
	}

	fn locate_known_block(&self, block_locator_hashes: Vec<H256>) -> Option<db::BestBlock> {
		let chain = self.chain.read();
		block_locator_hashes.into_iter()
			.filter_map(|hash| chain
				.storage_block_number(&hash)
				.map(|number| db::BestBlock {
					number: number,
					hash: hash,
				}))
			.nth(0)
	}

	fn server_worker<T: TaskExecutor>(queue_ready: Arc<Condvar>, queue: Arc<Mutex<ServerQueue>>, chain: ChainRef, executor: Arc<Mutex<T>>) {
		loop {
			let server_task = {
				let mut queue = queue.lock();
				if queue.is_stopping.load(Ordering::SeqCst) {
					break
				}
				queue.next_task()
					.map_or_else(|| {
						queue_ready.wait(&mut queue);
						queue.next_task()
					}, |next_task| Some(next_task))
			};

			match server_task {
				// has new task
				Some(server_task) => match server_task {
					// `getdata` => `notfound` + `block` + ... 
					(peer_index, ServerTask::ServeGetData(inventory)) => {
						let mut unknown_items: Vec<InventoryVector> = Vec::new();
						let mut new_tasks: Vec<ServerTask> = Vec::new();
						for item in inventory {
							match item.inv_type {
								InventoryType::MessageBlock => {
									match chain.read().storage_block_number(&item.hash) {
										Some(_) => new_tasks.push(ServerTask::ReturnBlock(item.hash.clone())),
										None => unknown_items.push(item),
									}
								},
								_ => (), // TODO: process other inventory types
							}
						}
						// respond with `notfound` message for unknown data
						if !unknown_items.is_empty() {
							trace!(target: "sync", "Going to respond with notfound with {} items to peer#{}", unknown_items.len(), peer_index);
							new_tasks.push(ServerTask::ReturnNotFound(unknown_items));
						}
						// schedule data responses
						if !new_tasks.is_empty() {
							trace!(target: "sync", "Going to respond with data with {} items to peer#{}", new_tasks.len(), peer_index);
							queue.lock().add_tasks(peer_index, new_tasks);
						}
					},
					// `inventory`
					(peer_index, ServerTask::ServeGetBlocks(best_block, hash_stop)) => {
						let storage_block_hash = chain.read().storage_block_hash(best_block.number);
						if let Some(hash) = storage_block_hash {
							// check that chain has not reorganized since task was queued
							if hash == best_block.hash {
								let mut inventory: Vec<InventoryVector> = Vec::new();
								let first_block_number = best_block.number + 1;
								let last_block_number = best_block.number + 500;
								// 500 hashes after best_block.number OR hash_stop OR blockchain end
								for block_number in first_block_number..last_block_number {
									match chain.read().storage_block_hash(block_number) {
										Some(ref block_hash) if block_hash == &hash_stop => break,
										None => break,
										Some(block_hash) => {
											inventory.push(InventoryVector {
												inv_type: InventoryType::MessageBlock,
												hash: block_hash,
											});
										},
									}
								}
								if !inventory.is_empty() {
									trace!(target: "sync", "Going to respond with inventory with {} items to peer#{}", inventory.len(), peer_index);
									executor.lock().execute(Task::SendInventory(peer_index, inventory));
								}
							}
						}
					},
					// `notfound`
					(peer_index, ServerTask::ReturnNotFound(inventory)) => {
						executor.lock().execute(Task::SendNotFound(peer_index, inventory));
					},
					// `block`
					(peer_index, ServerTask::ReturnBlock(block_hash)) => {
						let storage_block = chain.read().storage_block(&block_hash);
						if let Some(storage_block) = storage_block {
							executor.lock().execute(Task::SendBlock(peer_index, storage_block));
						}
					},
				},
				// no tasks after wake-up => stopping
				None => break,
			}
		}
	}
}

impl Drop for SynchronizationServer {
	fn drop(&mut self) {
		if let Some(join_handle) = self.worker_thread.take() {
			self.queue.lock().is_stopping.store(true, Ordering::SeqCst);
			self.queue_ready.notify_one();
			join_handle.join().expect("Clean shutdown.");
		}
	}
}

impl Server for SynchronizationServer {
	fn serve_getdata(&mut self, peer_index: usize, message: types::GetData) {
		self.queue.lock().add_task(peer_index, ServerTask::ServeGetData(message.inventory));
	}
 
	fn serve_getblocks(&mut self, peer_index: usize, message: types::GetBlocks) {
		if let Some(best_common_block) = self.locate_known_block(message.block_locator_hashes) {
			trace!(target: "sync", "Best common block with peer#{} is block#{}: {:?}", peer_index, best_common_block.number, best_common_block.hash);
			self.queue.lock().add_task(peer_index, ServerTask::ServeGetBlocks(best_common_block, message.hash_stop));
		}
		else {
			trace!(target: "sync", "No common blocks with peer#{}", peer_index);
		}
	}
}

impl ServerQueue {
	pub fn new(queue_ready: Arc<Condvar>) -> Self {
		ServerQueue {
			is_stopping: AtomicBool::new(false),
			queue_ready: queue_ready,
			peers_queue: VecDeque::new(),
			tasks_queue: HashMap::new(),
		}
	}

	pub fn next_task(&mut self) -> Option<(usize, ServerTask)> {
		self.peers_queue.pop_front()
			.map(|peer| {
				let (peer_task, no_tasks_left) = {
					let peer_tasks = self.tasks_queue.get_mut(&peer).expect("for each peer there is non-empty tasks queue");
					let peer_task = peer_tasks.pop_front().expect("for each peer there is non-empty tasks queue");
					(peer_task, peer_tasks.is_empty())
				};

				// remove if no tasks left || schedule otherwise
				if no_tasks_left {
					self.tasks_queue.remove(&peer);
				}
				else {
					self.peers_queue.push_back(peer);
				}
				(peer, peer_task)
			})
	}

	pub fn add_task(&mut self, peer_index: usize, task: ServerTask) {
		match self.tasks_queue.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				entry.get_mut().push_back(task);
			},
			Entry::Vacant(entry) => {
				let mut new_tasks = VecDeque::new();
				new_tasks.push_back(task);
				entry.insert(new_tasks);
				self.peers_queue.push_back(peer_index);
			}
		}
		self.queue_ready.notify_one();
	}

	pub fn add_tasks(&mut self, peer_index: usize, tasks: Vec<ServerTask>) {
		match self.tasks_queue.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				entry.get_mut().extend(tasks);
			},
			Entry::Vacant(entry) => {
				let mut new_tasks = VecDeque::new();
				new_tasks.extend(tasks);
				entry.insert(new_tasks);
				self.peers_queue.push_back(peer_index);
			}
		}
		self.queue_ready.notify_one();
	}
}

#[cfg(test)]
pub mod tests {
	use super::{Server, ServerTask};
	use message::types;
	use db;
	use std::mem::replace;

	pub struct DummyServer {
		tasks: Vec<(usize, ServerTask)>,
	}

	impl DummyServer {
		pub fn new() -> Self {
			DummyServer {
				tasks: Vec::new(),
			}
		}

		pub fn take_tasks(&mut self) -> Vec<(usize, ServerTask)> {
			replace(&mut self.tasks, Vec::new())
		}
	}

	impl Server for DummyServer {
		fn serve_getdata(&mut self, peer_index: usize, message: types::GetData) {
			self.tasks.push((peer_index, ServerTask::ServeGetData(message.inventory)));
		}

		fn serve_getblocks(&mut self, peer_index: usize, message: types::GetBlocks) {
			self.tasks.push((peer_index, ServerTask::ServeGetBlocks(db::BestBlock {
				number: 0,
				hash: message.block_locator_hashes[0].clone(),
			}, message.hash_stop)));
		}
	}
}
