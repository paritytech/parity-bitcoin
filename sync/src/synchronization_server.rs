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

/// Synchronization requests server
pub struct Server {
	queue_ready: Arc<Condvar>,
	queue: Arc<Mutex<ServerQueue>>,
	worker_thread: Option<thread::JoinHandle<()>>,
}

struct ServerQueue {
	is_stopping: AtomicBool,
	peers_queue: VecDeque<usize>,
	tasks_queue: HashMap<usize, VecDeque<ServerTask>>,
}

enum ServerTask {
	ServeData(Vec<InventoryVector>),
	ServeNotFound(Vec<InventoryVector>),
	ServeBlock(H256),
	ServeBlocksInventory(db::BestBlock, H256),
}

impl Drop for Server {
	fn drop(&mut self) {
		if let Some(join_handle) = self.worker_thread.take() {
			self.queue.lock().is_stopping.store(true, Ordering::SeqCst);
			self.queue_ready.notify_one();
			join_handle.join().expect("Clean shutdown.");
		}
	}
}

impl Server {
	pub fn new<T: TaskExecutor + Send + 'static>(chain: ChainRef, executor: Arc<Mutex<T>>) -> Self {
		let queue_ready = Arc::new(Condvar::new());
		let queue = Arc::new(Mutex::new(ServerQueue::new()));
		let mut server = Server {
			queue_ready: queue_ready.clone(),
			queue: queue.clone(),
			worker_thread: None,
		};
		server.worker_thread = Some(thread::spawn(move || {
			Server::server_worker(queue_ready, queue, chain, executor);
		}));
		server
	}

	pub fn serve_data(&self, peer_index: usize, inventory: Vec<InventoryVector>) {
		self.queue.lock().add_task(peer_index, ServerTask::ServeData(inventory));
	}
 
	pub fn serve_blocks_inventory(&self, peer_index: usize, best_common_block: db::BestBlock, hash_stop: H256) {
		self.queue.lock().add_task(peer_index, ServerTask::ServeBlocksInventory(best_common_block, hash_stop));
	}

	fn server_worker<T: TaskExecutor + Send + 'static>(queue_ready: Arc<Condvar>, queue: Arc<Mutex<ServerQueue>>, chain: ChainRef, executor: Arc<Mutex<T>>) {
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
					(peer_index, ServerTask::ServeData(inventory)) => {
						let mut unknown_items: Vec<InventoryVector> = Vec::new();
						let mut new_tasks: Vec<ServerTask> = Vec::new();
						for item in inventory {
							match item.inv_type {
								InventoryType::MessageBlock => {
									match chain.read().storage_block_number(&item.hash) {
										Some(_) => new_tasks.push(ServerTask::ServeBlock(item.hash.clone())),
										None => unknown_items.push(item),
									}
								},
								_ => (), // TODO: process other inventory types
							}
						}
						// respond with `notfound` message for unknown data
						if !unknown_items.is_empty() {
							new_tasks.push(ServerTask::ServeNotFound(unknown_items));
						}
						// schedule data responses
						if !new_tasks.is_empty() {
							queue.lock().add_tasks(peer_index, new_tasks);
						}
					},
					// `notfound`
					(peer_index, ServerTask::ServeNotFound(inventory)) => {
						executor.lock().execute(Task::SendNotFound(peer_index, inventory));
					},
					// `block`
					(peer_index, ServerTask::ServeBlock(block_hash)) => {
						if let Some(block) = chain.read().storage_block(&block_hash) {
							executor.lock().execute(Task::SendBlock(peer_index, block));
						}
					},
					// `inventory`
					(peer_index, ServerTask::ServeBlocksInventory(best_block, hash_stop)) => {
						if let Some(hash) = chain.read().storage_block_hash(best_block.number) {
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
									executor.lock().execute(Task::SendInventory(peer_index, inventory));
								}
							}
						}
					}
				},
				// no tasks after wake-up => stopping
				None => break,
			}
		}
	}
}

impl ServerQueue {
	pub fn new() -> Self {
		ServerQueue {
			is_stopping: AtomicBool::new(false),
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
	}
}
