use std::collections::{HashSet, HashMap, VecDeque};
use std::collections::hash_map::Entry;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use parking_lot::{Condvar, Mutex};
use message::{types, common};
use primitives::hash::H256;
use synchronization_executor::{Task, Executor};
use types::{BlockHeight, PeerIndex, RequestId, BlockHeight, ExecutorRef, MemoryPoolRef, StorageRef};

/// Synchronization server task
pub enum ServerTask {
	/// Serve 'getdata' request
	ServeGetData(PeerIndex, types::GetData),
	/// Serve reversed 'getdata' request
	ServeReversedGetData(PeerIndex, types::GetData, types::NotFound),
	/// Serve 'getblocks' request
	ServeGetBlocks(PeerIndex, types::GetBlocks),
	/// Serve 'getheaders' request
	ServeGetHeaders(PeerIndex, types::GetHeaders, RequestId),
	/// Serve 'mempool' request
	ServeMempool(PeerIndex),
	/// Serve 'getblocktxn' request
	ServeGetBlockTxn(PeerIndex, types::GetBlockTxn),
}

/// Synchronization server
pub trait Server {
	/// Execute single synchronization task
	fn execute(&self, peer_index: PeerIndex, task: ServerTask);
	/// Called when connection is closed
	fn on_disconnect(&self, peer_index: PeerIndex);
}

/// Synchronization server implementation
pub struct ServerImpl {
	/// New task is ready for execution in the queue
	queue_ready: Arc<Condvar>,
	/// Tasks queue
	queue: Arc<Mutex<ServerQueue>>,
	/// Worker thread join handle
	worker_thread: Option<thread::JoinHandle<()>>,
}

/// Synchronization server tasks queue
struct ServerQueue {
	/// True if server is stopping
	is_stopping: AtomicBool,
	/// New task is ready for execution in the queue
	queue_ready: Arc<Condvar>,
	/// Round-robin peers serving queue
	peers_queue: VecDeque<usize>,
	/// Scheduled tasks for each peer in execution order
	tasks_queue: HashMap<usize, VecDeque<ServerTask>>,
}

/// Synchronization server task executor
struct ServerTaskExecutor<TExecutor: Executor> {
	/// Storage reference
	storage: StorageRef,
	/// Memory pool reference
	memory_pool: MemoryPoolRef,
	/// Executor reference
	executor: ExecutorRef<TExecutor>,
}

impl ServerImpl {
	pub fn new<TExecutor: Executor>(storage: StorageRef, memory_pool: MemoryPoolRef, executor: ExecutorRef<TExecutor>) -> Self {
		let server_executor = ServerTaskExecutor::new(storage, memory_pool, executor);
		let queue_ready = Arc::new(Condvar::new());
		let queue = Arc::new(Mutex::new(ServerQueue::new(queue_ready.clone(), server_executor)));
		ServerImpl {
			queue_ready: queue_ready.clone(),
			queue: queue.clone(),
			worker_thread: Some(thread::spawn(move || ServerImpl::start(queue_ready, queue.downgrade(), server_executor))),
		}
	}

	fn start(queue_ready: Arc<Condvar>, queue: Arc<Mutex<ServerQueue>>, executor: ServerTaskExecutor) {
		loop {
			let task = {
				let mut queue = queue.lock();
				if queue.is_stopping.load(Ordering::SeqCst) {
					break;
				}

				queue.next_task()
					.or_else(|| {
						queue_ready.wait(&mut queue);
						queue.next_task()
					})
			};

			let task = match task {
				Some(server_task) => task,
				// no tasks after wake-up => stopping
				_ => continue,
			};

			executor.execute(task);
		}
	}
}

impl Server for ServerImpl {
	fn execute(&self, task: ServerTask) {
		self.queue.lock().add_task(task);
	}

	fn on_disconnect(&self, peer_index: PeerIndex) {
		self.queue.lock().remove_peer_tasks(peer_index);
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

	pub fn next_task(&mut self) -> Option<ServerTask> {
		self.peers_queue.pop_front()
			.map(|peer_index| {
				let (peer_task, is_last_peer_task) = {
					let peer_tasks = self.tasks_queue.get_mut(&peer_index)
						.expect("entry from tasks_queue is removed when empty; when empty, peer is removed from peers_queue; qed");
					let peer_task = peer_tasks.pop_front()
						.expect("entry from peer_tasks is removed when empty; when empty, peer is removed from peers_queue; qed");
					(peer_task, peer_tasks.is_empty())
				};				

				// remove if no tasks left || schedule otherwise
				if !is_last_peer_task {
					self.peers_queue.push_back(peer_index);
				} else {
					self.tasks_queue.remove(peer_index);
				}

				peer_task
			})
	}

	pub fn add_task(&mut self, task: ServerTask) {
		let peer = task.peer();
		match self.tasks_queue.entry(peer) {
			Entry::Occupied(mut entry) => {
				let add_to_peers_queue = entry.get().is_empty();
				entry.get_mut().push_back(task);
				if add_to_peers_queue {
					self.peers_queue.push_back(peer);
				}
			},
			Entry::Vacant(entry) => {
				let mut new_tasks = VecDeque::new();
				new_tasks.push_back(task);
				entry.insert(new_tasks);
				self.peers_queue.push_back(peer);
			}
		}

		self.queue_ready.notify_one();
	}

	pub fn remove_peer_tasks(&mut self, peer_index: PeerIndex) {
		if self.tasks_queue.remove(&peer_index).is_some() {
			let position = self.peers_queue.iter().position(|idx| idx == peer_index)
				.expect("tasks for peer are in tasks_queue; peers_queue and tasks_queue have same set of peers; qed");
			self.peers_queue.remove(position);
		}
	}
}

impl<TExecutor> ServerTaskExecutor where TExecutor: Executor {
	fn serve_get_data(&self, peer_index: PeerIndex, message: types::GetData) -> Option<ServerTask> {
		// getdata request is served by single item by just popping values from the back
		// of inventory vector
		// => to respond in given order, we have to reverse blocks inventory here
		message.inventory.reverse();
		// + while iterating by items, also accumulate unknown items to respond with notfound
		let notfound = types::NotFound { inventory: Vec::new(), };
		Some(ServerTask::ServeReversedGetData(peer_index, message, notfound))
	}

	fn serve_reversed_get_data(&self, peer_index: PeerIndex, mut message: types::GetData, mut notfound: types::NotFound) -> Option<ServerTask> {
		let next_item = match message.inventory.pop() {
			None => {
				if !notfound.inventory.is_empty() {
					trace!(target: "sync", "'getdata' from peer#{} container unknown items: {}", peer_index, notfound.short_info());
					self.executor.execute(Task::NotFound(peer_index, notfound));
				}
				return None;
			},
			Some(next_item) => next_item,
		};

		match next_item.inv_type {
			common::InventoryType::MessageTx => {
				// only transaction from memory pool can be requested
				if let Some(transaction) = self.memory_pool.read().read_by_hash(&next_item.hash) {
					let message = types::Tx::with_transaction(transaction);
					trace!(target: "sync", "'getblocks' tx response to peer#{} is ready: {}", peer_index, message.short_info());
					self.executor.execute(Task::SendTransaction(peer_index, message));
				} else {
					notfound.inventory.push(next_item);
				}
			},
			common::InventoryType::MessageBlock => {
				if let Some(block) = self.storage().block(next_item.hash.clone().into()) {
					let message = types::Block::with_block(block);
					trace!(target: "sync", "'getblocks' block response to peer#{} is ready: {}", peer_index, message.short_info());
					self.executor.execute(Task::SendBlock(peer_index, message));
				} else {
					notfound.inventory.push(next_item);
				}
			},
			common::InventoryType::MessageFilteredBlock => {
				if let Some(block) = self.storage().block(next_item.hash.clone().into()) {
					let message = self.peers.build_filtered_block(peer_index, block.into());
					trace!(target: "sync", "'getblocks' merkleblock response to peer#{} is ready: {}", peer_index, message.short_info());
					self.executor.execute(Task::SendMerkleBlock(peer_index, message));
				} else {
					notfound.inventory.push(next_item);
				}
			},
			common::InventoryType::MessageCompactBlock => {
				if let Some(block) = self.storage().block(next_item.hash.clone().into()) {
					let message = self.peers.build_compact_block(peer_index, block.into());
					trace!(target: "sync", "'getblocks' compactblock response to peer#{} is ready: {}", peer_index, message.short_info());
					self.executor.execute(Task::SendCompactBlock(peer_index, message));
				} else {
					notfound.inventory.push(next_item);
				}
			},
		}

		Some(ServerTask::ServeReversedGetData(peer_index, message, notfound))
	}

	fn serve_get_blocks(&self, peer_index: PeerIndex, message: types::GetBlocks) {
		if let Some(block_height) = self.locate_best_common_block(&message.hash_stop, &message.block_locator_hashes) {
			let inventory: Vec<_> = (block_height + 1..block_height + 1 + types::GetHeaders::MAX_BLOCKS)
				.map(|block_height| self.storage.block_hash(block_height))
				.take_while(|block_hash| block_hash != &message.hash_stop)
				.map(common::InventoryVector::block)
				.collect();
			// empty inventory messages are invalid according to regtests, while empty headers messages are valid
			if !inventory.is_empty() {
				let message = types::Inv::with_inventory(inventory);
				trace!(target: "sync", "'getblocks' response to peer#{} is ready: {}", peer_index, message.short_info());
				self.executor.execute(Task::SendInventory(peer_index, message));
			} else {
				trace!(target: "sync", "'getblocks' request from peer#{} is ignored as there are no new blocks for peer", peer_index);
			}
		} else {
			self.peers.misbehaving(peer_index, format!("Got 'getblocks' message without known blocks: {}", message.long_info()));
			return;
		}
	}

	fn serve_get_headers(&self, peer_index: PeerIndex, message: types::GetHeaders) {
		if let Some(block_height) = self.locate_best_common_block(&message.hash_stop, &message.block_locator_hashes) {
			let headers = (block_height + 1..block_height + 1 + types::GetHeaders::MAX_HEADERS)
				.map(|block_height| self.storage.block_hash(block_height))
				.take_while(|block_hash| block_hash != &message.hash_stop)
				.map(|block_hash| self.storage.block_header(block_hash.into()))
				.collect();
			// empty inventory messages are invalid according to regtests, while empty headers messages are valid
			let message = types::Headers {
				headers: headers,
			};
			trace!(target: "sync", "'getheaders' response to peer#{} is ready: {}", peer_index, message.short_info());
			self.executor.execute(Task::SendHeaders(peer_index, message));
		} else {
			self.peers.misbehaving(peer_index, format!("Got 'headers' message without known blocks: {}", message.long_info()));
			return;
		}
	}

	fn serve_mempool(&self, peer_index: PeerIndex) {
		let inventory: Vec<_> = self.memory_pool.read()
			.get_transactions_ids()
			.map(common::InventoryVector::tx)
			.collect();
		// empty inventory messages are invalid according to regtests, while empty headers messages are valid
		if !inventory.is_empty() {
			let message = types::Inv::with_inventory(inventory);
			trace!(target: "sync", "'mempool' response to peer#{} is ready: {}", peer_index, message.short_info());
			self.executor.execute(Task::SendInventory(peer_index, message));
		} else {
			trace!(target: "sync", "'mempool' request from peer#{} is ignored as pool is empty", peer_index);
		}
	}

	fn serve_get_block_txn(&self, peer_index: PeerIndex, message: types::GetBlockTxn) {
		// according to protocol documentation, we only should only respond
		// if requested block has been recently sent in 'cmpctblock'
		if !self.peers.is_sent_as(&message.request.blockhash, common::InventoryType::MessageCompactBlock) {
			self.peers.misbehaving(peer_index, format!("Got 'getblocktxn' message for non-sent block: {}", message.long_info()));
			return;
		}

		let block_transactions = match self.storage.block_transaction_hashes(message.request.blockhash.clone().into()) {
			None => {
				// we have checked that this block has been sent recently
				// => this is either some db error, or db has been pruned
				// => ignore
				warn!(target: "sync", "'getblocktxn' request from peer#{} is ignored as there are no transactions in storage", peer_index);
				return;
			},
			Some(block_transactions) => block_transactions,
		};

		let block_transactions_len = block_transactions.len();
		let requested_len = message.request.indexes.len();
		if requested_len > block_transactions_len {
			// peer has requested more transactions, than there are
			self.peers.misbehaving(peer_index, format!("Got 'getblocktxn' message with more transactions, than there are: {}", message.long_info()));
			return;
		}

		let mut requested_indexes = HashSet::new();
		let mut transactions = Vec::with_capacity(message.request.indexes.len());
		for transaction_index in message.request.indexes {
			if transaction_index >= block_transactions_len {
				// peer has requested index, larger than index of last transaction
				self.peers.misbehaving(peer_index, format!("Got 'getblocktxn' message with index, larger than index of last transaction: {}", message.long_info()));
				return;
			}
			if requested_indexes.insert(transaction_index) {
				// peer has requested same index several times
				self.peers.misbehaving(peer_index, format!("Got 'getblocktxn' message where same index has been requested several times: {}", message.long_info()));
				return;
			}

			if let Some(transaction) = self.storage.transaction(block_transactions[transaction_index]) {
				transactions.push(transaction);
			} else {
				// we have just got this hash using block_transactions_hashes
				// => this is either some db error, or db has been pruned
				// => we can not skip transactions, according to protocol description
				// => ignore
				warn!(target: "sync", "'getblocktxn' request from peer#{} is ignored as we have failed to find transaction {} in storage", peer_index, block_transactions[transaction_index].to_reversed_str());
				return;
			}
		}

		let message = types::BlockTxn {
			request: common::BlockTransactions {
				blockhash: message.request.blockhash,
				transactions: transactions,
			},
		};
		trace!(target: "sync", "'getblocktxn' response to peer#{} is ready: {}", peer_index, message.short_info());
		self.executor.execute(Task::SendBlockTxn(peer_index, message));
	}

	fn locate_best_common_block(&self, hash_stop: &H256, locator: &[H256]) -> Option<BlockHeight> {
		for block_hash in locator.iter().chain(&[hash_stop]) {
			if let Some(block_number) = self.storage.block_number(block_hash) {
				return Some(block_number);
			}

			// block with this hash is definitely not in the main chain (block_number has returned None)
			// but maybe it is in some fork? if so => we should find intersection with main chain
			// and this would be our best common block
			let mut block_hash = block_hash;
			loop {
				let block_header = match self.storage.block_header(block_hash.into()) {
					None => break,
					Some(block_header) => block_header,
				};

				if let Some(block_number) = self.storage.block_number(block_header.previous_block_hash.clone().into()) {
					return Some(block_number);
				}

				block_hash = block_header.previous_block_hash;
			}
		}
	}
}

impl<TExecutor> ServerTaskExecutor where TExecutor: Executor {
	fn execute(&self, task: ServerTask) -> Option<ServerTask> {
		match task {
			ServerTask::ServeGetData(peer_index, message) => self.serve_get_data(peer_index, message),
			ServerTask::ServeReversedGetData(peer_index, message) => self.serve_reversed_get_data(peer_index, message),
			ServerTask::ServeGetBlocks(peer_index, message) => self.serve_get_blocks(peer_index, message),
			ServerTask::ServeGetHeaders(peer_index, message) => self.serve_get_headers(peer_index, message),
			ServerTask::ServeMempool(peer_index) => self.serve_mempool(peer_index),
			ServerTask::ServeGetBlockTxn(peer_index, message) => self.serve_get_block_txn(peer_index, message),
		}
	}
}
