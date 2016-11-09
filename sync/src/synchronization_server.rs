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
	fn serve_getheaders(&mut self, peer_index: usize, message: types::GetHeaders);
	fn serve_mempool(&mut self, peer_index: usize);
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
	ServeGetHeaders(db::BestBlock, H256),
	ServeMempool,
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
		let storage = chain.storage();
		block_locator_hashes.into_iter()
			.filter_map(|hash| storage.block_number(&hash)
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
					}, Some)
			};

			match server_task {
				// has new task
				Some(server_task) => match server_task {
					// `getdata` => `notfound` + `block` + ...
					(peer_index, ServerTask::ServeGetData(inventory)) => {
						let mut unknown_items: Vec<InventoryVector> = Vec::new();
						let mut new_tasks: Vec<ServerTask> = Vec::new();
						{
							let chain = chain.read();
							let storage = chain.storage();
							for item in inventory {
								match item.inv_type {
									InventoryType::MessageBlock => {
										match storage.block_number(&item.hash) {
											Some(_) => new_tasks.push(ServerTask::ReturnBlock(item.hash.clone())),
											None => unknown_items.push(item),
										}
									},
									_ => (), // TODO: process other inventory types
								}
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
					// `getblocks` => `inventory`
					(peer_index, ServerTask::ServeGetBlocks(best_block, hash_stop)) => {
						let blocks_hashes = SynchronizationServer::blocks_hashes_after(&chain, &best_block, &hash_stop, 500);
						if !blocks_hashes.is_empty() {
							trace!(target: "sync", "Going to respond with inventory with {} items to peer#{}", blocks_hashes.len(), peer_index);
							let inventory = blocks_hashes.into_iter().map(|hash| InventoryVector {
								inv_type: InventoryType::MessageBlock,
								hash: hash,
							}).collect();
							executor.lock().execute(Task::SendInventory(peer_index, inventory));
						}
					},
					// `getheaders` => `headers`
					(peer_index, ServerTask::ServeGetHeaders(best_block, hash_stop)) => {
						// What if we have no common blocks with peer at all? Maybe drop connection or penalize peer?
						// https://github.com/ethcore/parity-bitcoin/pull/91#discussion_r86734568
						let blocks_hashes = SynchronizationServer::blocks_hashes_after(&chain, &best_block, &hash_stop, 2000);
						if !blocks_hashes.is_empty() {
							trace!(target: "sync", "Going to respond with blocks headers with {} items to peer#{}", blocks_hashes.len(), peer_index);
							let chain = chain.read();
							let storage = chain.storage();
							// TODO: read block_header only
							let blocks_headers = blocks_hashes.into_iter()
								.filter_map(|hash| storage.block(db::BlockRef::Hash(hash)).map(|block| block.block_header))
								.collect();
							executor.lock().execute(Task::SendHeaders(peer_index, blocks_headers));
						}
					},
					// `mempool` => `inventory`
					(peer_index, ServerTask::ServeMempool) => {
						let inventory: Vec<_> = chain.read()
							.memory_pool()
							.get_transactions_ids()
							.into_iter()
							.map(|hash| InventoryVector {
								inv_type: InventoryType::MessageTx,
								hash: hash,
							})
							.collect();
						if !inventory.is_empty() {
							trace!(target: "sync", "Going to respond with {} memory-pool transactions ids to peer#{}", inventory.len(), peer_index);
							executor.lock().execute(Task::SendInventory(peer_index, inventory));
						}
					},
					// `notfound`
					(peer_index, ServerTask::ReturnNotFound(inventory)) => {
						executor.lock().execute(Task::SendNotFound(peer_index, inventory));
					},
					// `block`
					(peer_index, ServerTask::ReturnBlock(block_hash)) => {
						let block = chain.read().storage().block(db::BlockRef::Hash(block_hash))
							.expect("we have checked that block exists in ServeGetData; db is append-only; qed");
						executor.lock().execute(Task::SendBlock(peer_index, block));
					},
				},
				// no tasks after wake-up => stopping
				None => break,
			}
		}
	}

	fn blocks_hashes_after(chain: &ChainRef, best_block: &db::BestBlock, hash_stop: &H256, max_hashes: u32) -> Vec<H256> {
		let mut hashes: Vec<H256> = Vec::new();
		let chain = chain.read();
		let storage = chain.storage();
		let storage_block_hash = storage.block_hash(best_block.number);
		if let Some(hash) = storage_block_hash {
			// check that chain has not reorganized since task was queued
			if hash == best_block.hash {
				let first_block_number = best_block.number + 1;
				let last_block_number = first_block_number + max_hashes;
				// `max_hashes` hashes after best_block.number OR hash_stop OR blockchain end
				for block_number in first_block_number..last_block_number {
					match storage.block_hash(block_number) {
						Some(ref block_hash) if block_hash == hash_stop => break,
						None => break,
						Some(block_hash) => {
							hashes.push(block_hash);
						},
					}
				}
			}
		}
		hashes
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

	fn serve_getheaders(&mut self, peer_index: usize, message: types::GetHeaders) {
		if let Some(best_common_block) = self.locate_known_block(message.block_locator_hashes) {
			trace!(target: "sync", "Best common block header with peer#{} is block#{}: {:?}", peer_index, best_common_block.number, best_common_block.hash);
			self.queue.lock().add_task(peer_index, ServerTask::ServeGetHeaders(best_common_block, message.hash_stop));
		}
		else {
			trace!(target: "sync", "No common blocks headers with peer#{}", peer_index);
		}
	}

	fn serve_mempool(&mut self, peer_index: usize) {
		self.queue.lock().add_task(peer_index, ServerTask::ServeMempool);
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
	use std::sync::Arc;
	use std::mem::replace;
	use parking_lot::{Mutex, RwLock};
	use db;
	use test_data;
	use primitives::hash::H256;
	use chain::{Transaction, RepresentH256};
	use message::types;
	use message::common::{InventoryVector, InventoryType};
	use synchronization_executor::Task;
	use synchronization_executor::tests::DummyTaskExecutor;
	use synchronization_chain::Chain;
	use super::{Server, ServerTask, SynchronizationServer};

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

		fn serve_getheaders(&mut self, peer_index: usize, message: types::GetHeaders) {
			self.tasks.push((peer_index, ServerTask::ServeGetHeaders(db::BestBlock {
				number: 0,
				hash: message.block_locator_hashes[0].clone(),
			}, message.hash_stop)));
		}

		fn serve_mempool(&mut self, peer_index: usize) {
			self.tasks.push((peer_index, ServerTask::ServeMempool));
		}
	}

	fn create_synchronization_server() -> (Arc<RwLock<Chain>>, Arc<Mutex<DummyTaskExecutor>>, SynchronizationServer) {
		let chain = Arc::new(RwLock::new(Chain::new(Arc::new(db::TestStorage::with_genesis_block()))));
		let executor = DummyTaskExecutor::new();
		let server = SynchronizationServer::new(chain.clone(), executor.clone());
		(chain, executor, server)
	}

	#[test]
	fn server_getdata_responds_notfound_when_block_not_found() {
		let (_, executor, mut server) = create_synchronization_server();
		// when asking for unknown block
		let inventory = vec![
			InventoryVector {
				inv_type: InventoryType::MessageBlock,
				hash: H256::default(),
			}
		];
		server.serve_getdata(0, types::GetData {
			inventory: inventory.clone(),
		});
		// => respond with notfound
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendNotFound(0, inventory)]);
	}

	#[test]
	fn server_getdata_responds_block_when_block_is_found() {
		let (_, executor, mut server) = create_synchronization_server();
		// when asking for known block
		let inventory = vec![
			InventoryVector {
				inv_type: InventoryType::MessageBlock,
				hash: test_data::genesis().hash(),
			}
		];
		server.serve_getdata(0, types::GetData {
			inventory: inventory.clone(),
		});
		// => respond with block
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendBlock(0, test_data::genesis())]);
	}

	#[test]
	fn server_getblocks_do_not_responds_inventory_when_synchronized() {
		let (_, executor, mut server) = create_synchronization_server();
		// when asking for blocks hashes
		let genesis_block_hash = test_data::genesis().hash();
		server.serve_getblocks(0, types::GetBlocks {
			version: 0,
			block_locator_hashes: vec![genesis_block_hash.clone()],
			hash_stop: H256::default(),
		});
		// => no response
		let tasks = DummyTaskExecutor::wait_tasks_for(executor, 100); // TODO: get rid of explicit timeout
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn server_getblocks_responds_inventory_when_have_unknown_blocks() {
		let (chain, executor, mut server) = create_synchronization_server();
		chain.write().insert_best_block(test_data::block_h1()).expect("Db write error");
		// when asking for blocks hashes
		server.serve_getblocks(0, types::GetBlocks {
			version: 0,
			block_locator_hashes: vec![test_data::genesis().hash()],
			hash_stop: H256::default(),
		});
		// => responds with inventory
		let inventory = vec![InventoryVector {
			inv_type: InventoryType::MessageBlock,
			hash: test_data::block_h1().hash(),
		}];
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendInventory(0, inventory)]);
	}

	#[test]
	fn server_getheaders_do_not_responds_headers_when_synchronized() {
		let (_, executor, mut server) = create_synchronization_server();
		// when asking for blocks hashes
		let genesis_block_hash = test_data::genesis().hash();
		server.serve_getheaders(0, types::GetHeaders {
			version: 0,
			block_locator_hashes: vec![genesis_block_hash.clone()],
			hash_stop: H256::default(),
		});
		// => no response
		let tasks = DummyTaskExecutor::wait_tasks_for(executor, 100); // TODO: get rid of explicit timeout
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn server_getheaders_responds_headers_when_have_unknown_blocks() {
		let (chain, executor, mut server) = create_synchronization_server();
		chain.write().insert_best_block(test_data::block_h1()).expect("Db write error");
		// when asking for blocks hashes
		server.serve_getheaders(0, types::GetHeaders {
			version: 0,
			block_locator_hashes: vec![test_data::genesis().hash()],
			hash_stop: H256::default(),
		});
		// => responds with headers
		let headers = vec![
			test_data::block_h1().block_header,
		];
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendHeaders(0, headers)]);
	}

	#[test]
	fn server_mempool_do_not_responds_inventory_when_empty_memory_pool() {
		let (_, executor, mut server) = create_synchronization_server();
		// when asking for memory pool transactions ids
		server.serve_mempool(0);
		// => no response
		let tasks = DummyTaskExecutor::wait_tasks_for(executor, 100); // TODO: get rid of explicit timeout
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn server_mempool_responds_inventory_when_non_empty_memory_pool() {
		let (chain, executor, mut server) = create_synchronization_server();
		// when memory pool is non-empty
		let transaction = Transaction::default();
		let transaction_hash = transaction.hash();
		chain.write().memory_pool_mut().insert_verified(transaction);
		// when asking for memory pool transactions ids
		server.serve_mempool(0);
		// => respond with inventory
		let inventory = vec![InventoryVector {
			inv_type: InventoryType::MessageTx,
			hash: transaction_hash,
		}];
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendInventory(0, inventory)]);
	}
}
