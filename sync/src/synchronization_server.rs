use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::collections::{VecDeque, HashMap};
use std::collections::hash_map::Entry;
use parking_lot::{Mutex, Condvar};
use message::common::{InventoryVector, InventoryType};
use db;
use chain::BlockHeader;
use primitives::hash::H256;
use synchronization_chain::{ChainRef, TransactionState};
use synchronization_executor::{Task, TaskExecutor};
use message::types;

/// Synchronization requests server trait
pub trait Server : Send + 'static {
	fn serve_getdata(&self, peer_index: usize, message: types::GetData);
	fn serve_getblocks(&self, peer_index: usize, message: types::GetBlocks);
	fn serve_getheaders(&self, peer_index: usize, message: types::GetHeaders, id: Option<u32>);
	fn serve_mempool(&self, peer_index: usize);
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
	tasks_queue: HashMap<usize, VecDeque<IndexedServerTask>>,
}

/// `ServerTask` index.
#[derive(Debug, PartialEq)]
pub enum ServerTaskIndex {
	/// `None` is used when response is sent out-of-order
	None,
	/// `Partial` is used when server needs to send more than one response for request.
	_Partial(u32),
	/// `Final` task task can be preceded by many `Partial` tasks with the same id.
	Final(u32),
}

impl ServerTaskIndex {
	pub fn raw(&self) -> Option<u32> {
		match *self {
			ServerTaskIndex::None => None,
			ServerTaskIndex::_Partial(id) | ServerTaskIndex::Final(id) => Some(id),
		}
	}

	pub fn _is_final(&self) -> bool {
		match *self {
			ServerTaskIndex::_Partial(_) => false,
			ServerTaskIndex::Final(_) => true,
			ServerTaskIndex::None => panic!("check with raw() before"),
		}
	}
}

/// Server tests together with unique id assigned to it
#[derive(Debug, PartialEq)]
pub struct IndexedServerTask {
	/// Task itself.
	task: ServerTask,
	/// Task id.
	id: ServerTaskIndex,
}

impl IndexedServerTask {
	fn new(task: ServerTask, id: ServerTaskIndex) -> Self {
		IndexedServerTask {
			task: task,
			id: id,
		}
	}
}

impl IndexedServerTask {
	fn ignore(id: u32) -> Self {
		IndexedServerTask::new(ServerTask::Ignore, ServerTaskIndex::Final(id))
	}
}

#[derive(Debug, PartialEq)]
pub enum ServerTask {
	ServeGetData(Vec<InventoryVector>),
	ServeGetBlocks(db::BestBlock, H256),
	ServeGetHeaders(db::BestBlock, H256),
	ServeMempool,
	ReturnNotFound(Vec<InventoryVector>),
	ReturnBlock(H256),
	Ignore,
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

	fn locate_known_block_hash(&self, block_locator_hashes: Vec<H256>) -> Option<db::BestBlock> {
		block_locator_hashes.into_iter()
			.filter_map(|hash| SynchronizationServer::locate_best_known_block_hash(&self.chain, &hash))
			.nth(0)
	}

	fn locate_known_block_header(&self, block_locator_hashes: Vec<H256>) -> Option<db::BestBlock> {
		self.locate_known_block_hash(block_locator_hashes)
	}

	fn server_worker<T: TaskExecutor>(queue_ready: Arc<Condvar>, queue: Arc<Mutex<ServerQueue>>, chain: ChainRef, executor: Arc<Mutex<T>>) {
		loop {
			let server_task = {
				let mut queue = queue.lock();
				if queue.is_stopping.load(Ordering::SeqCst) {
					break
				}

				queue.next_task()
					.or_else(|| {
						queue_ready.wait(&mut queue);
						queue.next_task()
					})
			};

			let (peer_index, indexed_task) = match server_task {
				Some((peer_index, indexed_task)) => (peer_index, indexed_task),
				// no tasks after wake-up => stopping or pausing
				_ => continue,
			};

			match indexed_task.task {
				// `getdata` => `notfound` + `block` + ...
				ServerTask::ServeGetData(inventory) => {
					let mut unknown_items: Vec<InventoryVector> = Vec::new();
					let mut new_tasks: Vec<IndexedServerTask> = Vec::new();
					assert_eq!(indexed_task.id.raw(), None);
					{
						let chain = chain.read();
						let storage = chain.storage();
						for item in inventory {
							match item.inv_type {
								InventoryType::MessageBlock => {
									match storage.block_number(&item.hash) {
										Some(_) => {
											let task = IndexedServerTask::new(ServerTask::ReturnBlock(item.hash.clone()), ServerTaskIndex::None);
											new_tasks.push(task);
										},
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
						let task = IndexedServerTask::new(ServerTask::ReturnNotFound(unknown_items), ServerTaskIndex::None);
						new_tasks.push(task);
					}
					// schedule data responses
					if !new_tasks.is_empty() {
						trace!(target: "sync", "Going to respond with data with {} items to peer#{}", new_tasks.len(), peer_index);
						queue.lock().add_tasks(peer_index, new_tasks);
					}
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// `getblocks` => `inventory`
				ServerTask::ServeGetBlocks(best_block, hash_stop) => {
					let blocks_hashes = SynchronizationServer::blocks_hashes_after(&chain, &best_block, &hash_stop, 500);
					if !blocks_hashes.is_empty() {
						trace!(target: "sync", "Going to respond with inventory with {} items to peer#{}", blocks_hashes.len(), peer_index);
						let inventory = blocks_hashes.into_iter().map(|hash| InventoryVector {
							inv_type: InventoryType::MessageBlock,
							hash: hash,
						}).collect();
						executor.lock().execute(Task::SendInventory(peer_index, inventory, indexed_task.id));
					} else {
						assert_eq!(indexed_task.id, ServerTaskIndex::None);
					}
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// `getheaders` => `headers`
				ServerTask::ServeGetHeaders(best_block, hash_stop) => {
					// What if we have no common blocks with peer at all? Maybe drop connection or penalize peer?
					// https://github.com/ethcore/parity-bitcoin/pull/91#discussion_r86734568
					let blocks_headers = SynchronizationServer::blocks_headers_after(&chain, &best_block, &hash_stop, 2000);
					if !blocks_headers.is_empty() {
						trace!(target: "sync", "Going to respond with blocks headers with {} items to peer#{}", blocks_headers.len(), peer_index);
						executor.lock().execute(Task::SendHeaders(peer_index, blocks_headers, indexed_task.id));
					} else if let Some(response_id) = indexed_task.id.raw() {
						executor.lock().execute(Task::Ignore(peer_index, response_id));
					}
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// `mempool` => `inventory`
				ServerTask::ServeMempool => {
					let inventory: Vec<_> = chain.read()
						.transactions_hashes_with_state(TransactionState::InMemory)
						.into_iter()
						.map(|hash| InventoryVector {
							inv_type: InventoryType::MessageTx,
							hash: hash,
						})
						.collect();
					if !inventory.is_empty() {
						trace!(target: "sync", "Going to respond with {} memory-pool transactions ids to peer#{}", inventory.len(), peer_index);
						executor.lock().execute(Task::SendInventory(peer_index, inventory, indexed_task.id));
					} else {
						assert_eq!(indexed_task.id, ServerTaskIndex::None);
					}
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// `notfound`
				ServerTask::ReturnNotFound(inventory) => {
					executor.lock().execute(Task::SendNotFound(peer_index, inventory, indexed_task.id));
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// `block`
				ServerTask::ReturnBlock(block_hash) => {
					let block = chain.read().storage().block(db::BlockRef::Hash(block_hash))
						.expect("we have checked that block exists in ServeGetData; db is append-only; qed");
					executor.lock().execute(Task::SendBlock(peer_index, block, indexed_task.id));
					// inform that we have processed task for peer
					queue.lock().task_processed(peer_index);
				},
				// ignore
				ServerTask::Ignore => {
					let response_id = indexed_task.id.raw().expect("do not schedule redundant ignore task");
					executor.lock().execute(Task::Ignore(peer_index, response_id));
					queue.lock().task_processed(peer_index);
				},
			}
		}
	}

	fn blocks_hashes_after(chain: &ChainRef, best_block: &db::BestBlock, hash_stop: &H256, max_hashes: u32) -> Vec<H256> {
		let chain = chain.read();
		// check that chain has not reorganized since task was queued
		if chain.block_hash(best_block.number).map(|h| h != best_block.hash).unwrap_or(true) {
			return Vec::new();
		}

		let first_block_number = best_block.number + 1;
		let last_block_number = first_block_number + max_hashes;
		// `max_hashes` hashes after best_block.number OR hash_stop OR blockchain end
		(first_block_number..last_block_number).into_iter()
			.map(|number| chain.block_hash(number))
			.take_while(|hash| hash.is_some())
			.map(|hash| hash.unwrap())
			.take_while(|hash| hash != hash_stop)
			.collect()
	}

	fn blocks_headers_after(chain: &ChainRef, best_block: &db::BestBlock, hash_stop: &H256, max_hashes: u32) -> Vec<BlockHeader> {
		let chain = chain.read();
		// check that chain has not reorganized since task was queued
		if chain.block_hash(best_block.number).map(|h| h != best_block.hash).unwrap_or(true) {
			return Vec::new();
		}

		let first_block_number = best_block.number + 1;
		let last_block_number = first_block_number + max_hashes;
		// `max_hashes` hashes after best_block.number OR hash_stop OR blockchain end
		(first_block_number..last_block_number).into_iter()
			.map(|number| chain.block_header_by_number(number))
			.take_while(|header| header.is_some())
			.map(|header| header.unwrap())
			.take_while(|header| &header.hash() != hash_stop)
			.collect()
	}


	fn locate_best_known_block_hash(chain: &ChainRef, hash: &H256) -> Option<db::BestBlock> {
		let chain = chain.read();
		match chain.block_number(hash) {
			Some(number) => Some(db::BestBlock {
				number: number,
				hash: hash.clone(),
			}),
			// block with hash is not in the main chain (block_number has returned None)
			// but maybe it is in some fork? if so => we should find intersection with main chain
			// and this would be our best common block
			None => chain.block_header_by_hash(hash)
				.and_then(|block| {
					let mut current_block_hash = block.previous_header_hash;
					loop {
						if let Some(block_number) = chain.block_number(&current_block_hash) {
							return Some(db::BestBlock {
								number: block_number,
								hash: current_block_hash,
							});
						}

						match chain.block_header_by_hash(&current_block_hash) {
							Some(current_block_header) => current_block_hash = current_block_header.previous_header_hash,
							None => return None,
						}
					}
				}),
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
	fn serve_getdata(&self, peer_index: usize, message: types::GetData) {
		let task = IndexedServerTask::new(ServerTask::ServeGetData(message.inventory), ServerTaskIndex::None);
		self.queue.lock().add_task(peer_index, task);
	}

	fn serve_getblocks(&self, peer_index: usize, message: types::GetBlocks) {
		if let Some(best_common_block) = self.locate_known_block_hash(message.block_locator_hashes) {
			trace!(target: "sync", "Best common block with peer#{} is block#{}: {:?}", peer_index, best_common_block.number, best_common_block.hash);
			let task = IndexedServerTask::new(ServerTask::ServeGetBlocks(best_common_block, message.hash_stop), ServerTaskIndex::None);
			self.queue.lock().add_task(peer_index, task);
		}
		else {
			trace!(target: "sync", "No common blocks with peer#{}", peer_index);
		}
	}

	fn serve_getheaders(&self, peer_index: usize, message: types::GetHeaders, id: Option<u32>) {
		if let Some(best_common_block) = self.locate_known_block_header(message.block_locator_hashes) {
			trace!(target: "sync", "Best common block header with peer#{} is block#{}: {:?}", peer_index, best_common_block.number, best_common_block.hash.to_reversed_str());
			let server_task_index = id.map_or_else(|| ServerTaskIndex::None, |id| ServerTaskIndex::Final(id));
			let task = IndexedServerTask::new(ServerTask::ServeGetHeaders(best_common_block, message.hash_stop), server_task_index);
			self.queue.lock().add_task(peer_index, task);
		}
		else {
			trace!(target: "sync", "No common blocks headers with peer#{}", peer_index);
			if let Some(id) = id {
				self.queue.lock().add_task(peer_index, IndexedServerTask::ignore(id));
			}
		}
	}

	fn serve_mempool(&self, peer_index: usize) {
		let task = IndexedServerTask::new(ServerTask::ServeMempool, ServerTaskIndex::None);
		self.queue.lock().add_task(peer_index, task);
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

	pub fn next_task(&mut self) -> Option<(usize, IndexedServerTask)> {
		self.peers_queue.pop_front()
			.map(|peer| {
				let (peer_task, no_tasks_left) = {
					let peer_tasks = self.tasks_queue.get_mut(&peer).expect("for each peer there is non-empty tasks queue");
					let peer_task = peer_tasks.pop_front().expect("for each peer there is non-empty tasks queue");
					(peer_task, peer_tasks.is_empty())
				};

				// remove if no tasks left || schedule otherwise
				if !no_tasks_left {
					self.peers_queue.push_back(peer);
				}
				(peer, peer_task)
			})
	}

	pub fn task_processed(&mut self, peer_index: usize) {
		if let Entry::Occupied(tasks_entry) = self.tasks_queue.entry(peer_index) {
			if !tasks_entry.get().is_empty() {
				return;
			}
			tasks_entry.remove_entry();
		}
	}

	pub fn add_task(&mut self, peer_index: usize, task: IndexedServerTask) {
		match self.tasks_queue.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				let add_to_peers_queue = entry.get().is_empty();
				entry.get_mut().push_back(task);
				if add_to_peers_queue {
					self.peers_queue.push_back(peer_index);
				}
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

	pub fn add_tasks(&mut self, peer_index: usize, tasks: Vec<IndexedServerTask>) {
		match self.tasks_queue.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				let add_to_peers_queue = entry.get().is_empty();
				entry.get_mut().extend(tasks);
				if add_to_peers_queue {
					self.peers_queue.push_back(peer_index);
				}
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
	use super::{Server, ServerTask, SynchronizationServer, ServerTaskIndex};

	pub struct DummyServer {
		tasks: Mutex<Vec<(usize, ServerTask)>>,
	}

	impl DummyServer {
		pub fn new() -> Self {
			DummyServer {
				tasks: Mutex::new(Vec::new()),
			}
		}

		pub fn take_tasks(&self) -> Vec<(usize, ServerTask)> {
			replace(&mut *self.tasks.lock(), Vec::new())
		}
	}

	impl Server for DummyServer {
		fn serve_getdata(&self, peer_index: usize, message: types::GetData) {
			self.tasks.lock().push((peer_index, ServerTask::ServeGetData(message.inventory)));
		}

		fn serve_getblocks(&self, peer_index: usize, message: types::GetBlocks) {
			self.tasks.lock().push((peer_index, ServerTask::ServeGetBlocks(db::BestBlock {
				number: 0,
				hash: message.block_locator_hashes[0].clone(),
			}, message.hash_stop)));
		}

		fn serve_getheaders(&self, peer_index: usize, message: types::GetHeaders, _id: Option<u32>) {
			self.tasks.lock().push((peer_index, ServerTask::ServeGetHeaders(db::BestBlock {
				number: 0,
				hash: message.block_locator_hashes[0].clone(),
			}, message.hash_stop)));
		}

		fn serve_mempool(&self, peer_index: usize) {
			self.tasks.lock().push((peer_index, ServerTask::ServeMempool));
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
		let (_, executor, server) = create_synchronization_server();
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
		assert_eq!(tasks, vec![Task::SendNotFound(0, inventory, ServerTaskIndex::None)]);
	}

	#[test]
	fn server_getdata_responds_block_when_block_is_found() {
		let (_, executor, server) = create_synchronization_server();
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
		assert_eq!(tasks, vec![Task::SendBlock(0, test_data::genesis(), ServerTaskIndex::None)]);
	}

	#[test]
	fn server_getblocks_do_not_responds_inventory_when_synchronized() {
		let (_, executor, server) = create_synchronization_server();
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
		let (chain, executor, server) = create_synchronization_server();
		chain.write().insert_best_block(test_data::block_h1().hash(), &test_data::block_h1()).expect("Db write error");
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
		assert_eq!(tasks, vec![Task::SendInventory(0, inventory, ServerTaskIndex::None)]);
	}

	#[test]
	fn server_getheaders_do_not_responds_headers_when_synchronized() {
		let (_, executor, server) = create_synchronization_server();
		// when asking for blocks hashes
		let genesis_block_hash = test_data::genesis().hash();
		let dummy_id = 6;
		server.serve_getheaders(0, types::GetHeaders {
			version: 0,
			block_locator_hashes: vec![genesis_block_hash.clone()],
			hash_stop: H256::default(),
		}, Some(dummy_id));
		// => no response
		let tasks = DummyTaskExecutor::wait_tasks_for(executor, 100); // TODO: get rid of explicit timeout
		assert_eq!(tasks, vec![Task::Ignore(0, dummy_id)]);
	}

	#[test]
	fn server_getheaders_responds_headers_when_have_unknown_blocks() {
		let (chain, executor, server) = create_synchronization_server();
		chain.write().insert_best_block(test_data::block_h1().hash(), &test_data::block_h1()).expect("Db write error");
		// when asking for blocks hashes
		let dummy_id = 0;
		server.serve_getheaders(0, types::GetHeaders {
			version: 0,
			block_locator_hashes: vec![test_data::genesis().hash()],
			hash_stop: H256::default(),
		}, Some(dummy_id));
		// => responds with headers
		let headers = vec![
			test_data::block_h1().block_header,
		];
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendHeaders(0, headers, ServerTaskIndex::Final(dummy_id))]);
	}

	#[test]
	fn server_mempool_do_not_responds_inventory_when_empty_memory_pool() {
		let (_, executor, server) = create_synchronization_server();
		// when asking for memory pool transactions ids
		server.serve_mempool(0);
		// => no response
		let tasks = DummyTaskExecutor::wait_tasks_for(executor, 100); // TODO: get rid of explicit timeout
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn server_mempool_responds_inventory_when_non_empty_memory_pool() {
		let (chain, executor, server) = create_synchronization_server();
		// when memory pool is non-empty
		let transaction = Transaction::default();
		let transaction_hash = transaction.hash();
		chain.write().insert_verified_transaction(transaction);
		// when asking for memory pool transactions ids
		server.serve_mempool(0);
		// => respond with inventory
		let inventory = vec![InventoryVector {
			inv_type: InventoryType::MessageTx,
			hash: transaction_hash,
		}];
		let tasks = DummyTaskExecutor::wait_tasks(executor);
		assert_eq!(tasks, vec![Task::SendInventory(0, inventory, ServerTaskIndex::None)]);
	}
}
