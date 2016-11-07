use std::thread;
use std::sync::Arc;
use std::cmp::{min, max};
use std::collections::{HashMap, VecDeque};
use std::collections::hash_map::Entry;
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
use futures::{BoxFuture, Future, finished};
use futures::stream::Stream;
use tokio_core::reactor::{Handle, Interval};
use futures_cpupool::CpuPool;
use db;
use chain::{Block, RepresentH256};
use primitives::hash::H256;
use synchronization_peers::Peers;
#[cfg(test)] use synchronization_peers::{Information as PeersInformation};
use synchronization_chain::{ChainRef, BlockState, InventoryIntersection};
#[cfg(test)]
use synchronization_chain::{Information as ChainInformation};
use verification::{ChainVerifier, Error as VerificationError, Verify};
use synchronization_executor::{Task, TaskExecutor};
use synchronization_manager::{manage_synchronization_peers, MANAGEMENT_INTERVAL_MS};
use time;
use std::time::Duration;

///! Blocks synchronization process:
///!
///! When new peer is connected:
///! 1) send `inventory` message with full block locator hashes (see `LocalNode`)
///!
///! on_new_blocks_inventory: When `inventory` message is received from peer:
///! 1) queue_intersection = intersect(queue, inventory)
///! 2) if !queue_intersection.is_empty(): ===> responded with blocks within sync window
///! 2.1) remember peer as useful
///! 2.2) inventory_rest = inventory - queue_intersection
///! 2.3) if inventory_rest.is_empty(): ===> no new unknown blocks in inventory
///! 2.3.1) stop (2.3)
///! 2.4) if !inventory_rest.is_empty(): ===> has new unknown blocks in inventory
///! 2.4.1) queue_rest = queue after intersection
///! 2.4.2) if queue_rest.is_empty(): ===> has new unknown blocks in inventory, no fork
///! 2.4.2.1) scheduled_blocks.append(inventory_rest)
///! 2.4.2.2) stop (2.4.2)
///! 2.4.3) if !queue_rest.is_empty(): ===> has new unknown blocks in inventory, fork
///! 2.4.3.1) scheduled_blocks.append(inventory_rest)
///! 2.4.3.2) stop (2.4.3)
///! 2.4.3) stop (2.4)
///! 2.5) stop (2)
///! 3) if queue_intersection.is_empty(): ===> responded with out-of-sync-window blocks
///! 3.1) last_known_block = inventory.last(b => b.is_known())
///! 3.2) if last_known_block == None: ===> we know nothing about these blocks & we haven't asked for these
///! 3.2.1) if !synchronizing => remember peer as useful + ask for blocks
///! 3.2.1) if synchronizing => peer will be excluded later by management thread
///! 3.2.2) stop (3.2)
///! 3.3) if last_known_block == last(inventory): ===> responded with all-known-blocks
///! 3.3.1) if syncing, remember peer as useful (possibly had failures before && have been excluded from sync)
///! 3.3.2) stop (3.3)
///! 3.4) if last_known_block in the middle of inventory: ===> responded with forked blocks
///! 3.4.1) remember peer as useful
///! 3.4.2) inventory_rest = inventory after last_known_block
///! 3.4.3) scheduled_blocks.append(inventory_rest)
///! 3.4.4) stop (3.4)
///! 3.5) stop (3)
///!
///! on_peer_block: After receiving `block` message:
///! 1) if block_state(block) in (Verifying, Stored): ===> late delivery
///! 1.1) remember peer as useful
///! 1.2) stop (1)
///! 2) if block_state(block) in (Scheduled, Requested): ===> future/on-time delivery
///! 2.1) remember peer as useful
///! 2.2) if block_state(block.parent) in (Verifying, Stored): ===> we can proceed with verification
///! 2.2.1) remove block from current queue (Verifying || Stored)
///! 2.2.2) append block to the verification queue
///! 2.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 2.2.4) try to verify orphan blocks
///! 2.2.5) stop (2.2)
///! 2.3) if block_state(block.parent) in (Requested, Scheduled): ===> we have found an orphan block
///! 2.3.1) remove block from current queue (Verifying || Stored)
///! 2.3.2) append block to the orphans
///! 2.3.3) stop (2.3)
///! 2.4) if block_state(block.parent) == Unknown: ===> bad block found
///! 2.4.1) remove block from current queue (Verifying || Stored)
///! 2.4.2) stop (2.4)
///! 2.5) stop (2)
///! 3) if block_state(block) == Unknown: ===> maybe we are on-top of chain && new block is announced?
///! 3.1) if block_state(block.parent_hash) == Unknown: ===> we do not know parent
///! 3.1.1) ignore this block
///! 3.1.2) stop (3.1)
///! 3.2) if block_state(block.parent_hash) in (Verifying, Stored): ===> fork found, can verify
///! 3.2.1) ask peer for best inventory (after this block)
///! 3.2.2) append block to verifying queue
///! 3.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 3.2.4) stop (3.2)
///! 3.3) if block_state(block.parent_hash) in (Requested, Scheduled): ===> fork found, add as orphan
///! 3.3.1) ask peer for best inventory (after this block)
///! 3.3.2) append block to orphan
///! 3.3.3) stop (3.3)
///! 3.4) stop (2)
///! + if no blocks left in scheduled + requested queue => we are saturated => ask all peers for inventory & forget
///!
///! execute_synchronization_tasks: After receiving `inventory` message OR receiving `block` message OR when management thread schedules tasks:
///! 1) if there are blocks in `scheduled` queue AND we can fit more blocks into memory: ===> ask for blocks
///! 1.1) select idle peers
///! 1.2) for each idle peer: query chunk of blocks from `scheduled` queue
///! 1.3) move requested blocks from `scheduled` to `requested` queue
///! 1.4) mark idle peers as active
///! 1.5) stop (1)
///! 2) if `scheduled` queue is not yet saturated: ===> ask for new blocks hashes
///! 2.1) for each idle peer: send shortened `getblocks` message
///! 2.2) 'forget' idle peers => they will be added again if respond with inventory
///! 2.3) stop (2)
///!
///! manage_synchronization_peers: When management thread awakes:
///! 1) for peer in active_peers.where(p => now() - p.last_request_time() > failure_interval):
///! 1.1) return all peer' tasks to the tasks pool (TODO: not implemented currently!!!)
///! 1.2) increase # of failures for this peer
///! 1.3) if # of failures > max_failures: ===> super-bad peer
///! 1.3.1) forget peer
///! 1.3.3) stop (1.3)
///! 1.4) if # of failures <= max_failures: ===> bad peer
///! 1.4.1) move peer to idle pool
///! 1.4.2) stop (1.4)
///! 2) schedule tasks from pool (if any)
///!
///! on_block_verification_success: When verification completes scuccessfully:
///! 1) if block_state(block) != Verifying: ===> parent verification failed
///! 1.1) stop (1)
///! 2) remove from verifying queue
///! 3) insert to the db
///!
///! on_block_verification_error: When verification completes with an error:
///! 1) remove block from verification queue
///! 2) remove all known children from all queues [so that new `block` messages will be ignored in on_peer_block.3.1.1] (TODO: not implemented currently!!!)
///!
///!

/// Approximate maximal number of blocks hashes in scheduled queue.
const MAX_SCHEDULED_HASHES: u32 = 4 * 1024;
/// Approximate maximal number of blocks hashes in requested queue.
const MAX_REQUESTED_BLOCKS: u32 = 256;
/// Approximate maximal number of blocks in verifying queue.
const MAX_VERIFYING_BLOCKS: u32 = 256;
/// Minimum number of blocks to request from peer
const MIN_BLOCKS_IN_REQUEST: u32 = 32;
/// Maximum number of blocks to request from peer
const MAX_BLOCKS_IN_REQUEST: u32 = 128;

/// Synchronization state
#[derive(Debug, Clone, Copy)]
pub enum State {
	Synchronizing(f64, u32),
	Saturated,
}

/// Information on current synchronization state.
#[cfg(test)]
#[derive(Debug)]
pub struct Information {
	/// Current synchronization state.
	pub state: State,
	/// Information on synchronization peers.
	pub peers: PeersInformation,
	/// Current synchronization chain inormation.
	pub chain: ChainInformation,
	/// Number of currently orphaned blocks.
	pub orphaned: usize,
}

/// Verification thread tasks
enum VerificationTask {
	/// Verify single block
	VerifyBlock(Block),
	/// Stop verification thread
	Stop,
}

/// Synchronization client trait
pub trait Client : Send + 'static {
	fn best_block(&self) -> db::BestBlock;
	fn on_new_blocks_inventory(&mut self, peer_index: usize, peer_hashes: Vec<H256>);
	fn on_peer_block(&mut self, peer_index: usize, block: Block);
	fn on_peer_disconnected(&mut self, peer_index: usize);
	fn on_block_verification_success(&mut self, block: Block);
	fn on_block_verification_error(&mut self, err: &VerificationError, hash: &H256);
}

/// Synchronization client configuration options.
pub struct Config {
	/// Number of threads to allocate in synchronization CpuPool.
	pub threads_num: usize,
	/// Do not verify incoming blocks before inserting to db.
	pub skip_verification: bool,
}

/// Synchronization client.
pub struct SynchronizationClient<T: TaskExecutor> {
	/// Synchronization state.
	state: State,
	/// Cpu pool.
	pool: CpuPool,
	/// Sync management worker.
	management_worker: Option<BoxFuture<(), ()>>,
	/// Synchronization peers.
	peers: Peers,
	/// Task executor.
	executor: Arc<Mutex<T>>,
	/// Chain reference.
	chain: ChainRef,
	/// Blocks from requested_hashes, but received out-of-order.
	orphaned_blocks: HashMap<H256, Vec<(H256, Block)>>,
	/// Verification work transmission channel.
	verification_work_sender: Option<Sender<VerificationTask>>,
	/// Verification thread.
	verification_worker_thread: Option<thread::JoinHandle<()>>,
}

impl Default for Config {
	fn default() -> Self {
		Config {
			threads_num: 4,
			skip_verification: false,
		}
	}
}

impl State {
	pub fn is_synchronizing(&self) -> bool {
		match self {
			&State::Synchronizing(_, _) => true,
			_ => false,
		}
	}
}

impl<T> Drop for SynchronizationClient<T> where T: TaskExecutor {
	fn drop(&mut self) {
		if let Some(join_handle) = self.verification_worker_thread.take() {
			// ignore send error here <= destructing anyway
			let _ = self.verification_work_sender
				.take()
				.expect("Some(join_handle) => Some(verification_work_sender)")
				.send(VerificationTask::Stop);
			join_handle.join().expect("Clean shutdown.");
		}
	}
}

impl<T> Client for SynchronizationClient<T> where T: TaskExecutor {
	/// Get best known block
	fn best_block(&self) -> db::BestBlock {
		self.chain.read().best_block()
	}

	/// Try to queue synchronization of unknown blocks when new inventory is received.
	fn on_new_blocks_inventory(&mut self, peer_index: usize, peer_hashes: Vec<H256>) {
		self.process_new_blocks_inventory(peer_index, peer_hashes);
		self.execute_synchronization_tasks();
	}

	/// Process new block.
	fn on_peer_block(&mut self, peer_index: usize, block: Block) {
		let block_hash = block.hash();

		// update peers to select next tasks
		self.peers.on_block_received(peer_index, &block_hash);

		self.process_peer_block(peer_index, block_hash, block);
		self.execute_synchronization_tasks();
	}

	/// Peer disconnected.
	fn on_peer_disconnected(&mut self, peer_index: usize) {
		// when last peer is disconnected, reset, but let verifying blocks be verified
		if self.peers.on_peer_disconnected(peer_index) {
			self.switch_to_saturated_state(false);
		}
	}

	/// Process successful block verification
	fn on_block_verification_success(&mut self, block: Block) {
		{
			let mut chain = self.chain.write();

			// remove block from verification queue
			chain.remove_block_with_state(&block.hash(), BlockState::Verifying);

			// insert to storage
			chain.insert_best_block(block)
				.expect("Error inserting to db.");
		}

		// continue with synchronization
		self.execute_synchronization_tasks();
	}

	/// Process failed block verification
	fn on_block_verification_error(&mut self, err: &VerificationError, hash: &H256) {
		warn!(target: "sync", "Block {:?} verification failed with error {:?}", hash, err);

		{
			let mut chain = self.chain.write();

			// remove block from verification queue
			chain.remove_block_with_state(&hash, BlockState::Verifying);
		}

		// start new tasks
		self.execute_synchronization_tasks();
	}
}

impl<T> SynchronizationClient<T> where T: TaskExecutor {
	/// Create new synchronization window
	pub fn new(config: Config, handle: &Handle, executor: Arc<Mutex<T>>, chain: ChainRef) -> Arc<Mutex<Self>> {
		let sync = Arc::new(Mutex::new(
			SynchronizationClient {
				state: State::Saturated,
				peers: Peers::new(),
				pool: CpuPool::new(config.threads_num),
				management_worker: None,
				executor: executor,
				chain: chain.clone(),
				orphaned_blocks: HashMap::new(),
				verification_work_sender: None,
				verification_worker_thread: None,
			}
		));

		if !config.skip_verification {
			let (verification_work_sender, verification_work_receiver) = channel();
			let csync = sync.clone();
			let mut lsync = sync.lock();
			let storage = chain.read().storage();
			lsync.verification_work_sender = Some(verification_work_sender);
			lsync.verification_worker_thread = Some(thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					SynchronizationClient::verification_worker_proc(csync, storage, verification_work_receiver)
				})
				.expect("Error creating verification thread"));
		}

		// TODO: start management worker only when synchronization is started
		//       currently impossible because there is no way to call Interval::new with Remote && Handle is not-Send
		{
			let csync = Arc::downgrade(&sync);
			let mut sync = sync.lock();
			let management_worker = Interval::new(Duration::from_millis(MANAGEMENT_INTERVAL_MS), handle)
				.expect("Failed to create interval")
				.and_then(move |_| {
					let client = match csync.upgrade() {
						Some(client) => client,
						None => return Ok(()),
					};
					let mut client = client.lock();
					if client.state.is_synchronizing() {
						manage_synchronization_peers(&mut client.peers);
						client.execute_synchronization_tasks();
					}
					Ok(())
				})
				.for_each(|_| Ok(()))
				.then(|_| finished::<(), ()>(()))
				.boxed();
			sync.management_worker = Some(sync.pool.spawn(management_worker).boxed());
		}

		sync
	}

	/// Get information on current synchronization state.
	#[cfg(test)]
	pub fn information(&self) -> Information {
		Information {
			state: self.state,
			peers: self.peers.information(),
			chain: self.chain.read().information(),
			orphaned: self.orphaned_blocks.len(),
		}
	}

	/// Process new blocks inventory
	fn process_new_blocks_inventory(&mut self, peer_index: usize, mut inventory: Vec<H256>) {
		let mut chain = self.chain.write();
		match chain.intersect_with_inventory(&inventory) {
			InventoryIntersection::NoKnownBlocks(_) if self.state.is_synchronizing() => (),
			InventoryIntersection::DbAllBlocksKnown => {
				if self.state.is_synchronizing() {
					// remember peer as useful
					self.peers.insert(peer_index);
				}
			},
			InventoryIntersection::InMemoryNoNewBlocks => {
				// remember peer as useful
				self.peers.insert(peer_index);
			},
			InventoryIntersection::InMemoryMainNewBlocks(new_block_index)
				| InventoryIntersection::InMemoryForkNewBlocks(new_block_index)
				| InventoryIntersection::DbForkNewBlocks(new_block_index)
				| InventoryIntersection::NoKnownBlocks(new_block_index) => {
				// schedule new blocks
				let new_blocks_hashes = inventory.split_off(new_block_index);
				chain.schedule_blocks_hashes(new_blocks_hashes);
				// remember peer as useful
				self.peers.insert(peer_index);
				// switch to synchronization state
				if !self.state.is_synchronizing() {
					self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);
				}
			}
		}
	}

	/// Process new peer block
	fn process_peer_block(&mut self, peer_index: usize, block_hash: H256, block: Block) {
		let switch_to_saturated = {
			let mut chain = self.chain.write();
			match chain.block_state(&block_hash) {
				BlockState::Verifying | BlockState::Stored => {
					// remember peer as useful
					self.peers.insert(peer_index);
				},
				BlockState::Unknown | BlockState::Scheduled | BlockState::Requested => {
					// remove block from current queue
					chain.remove_block(&block_hash);
					// check parent block state
					match chain.block_state(&block.block_header.previous_header_hash) {
						BlockState::Unknown => (),
						BlockState::Verifying | BlockState::Stored => {
							// remember peer as useful
							self.peers.insert(peer_index);
							// schedule verification
							let mut blocks: VecDeque<(H256, Block)> = VecDeque::new();
							blocks.push_back((block_hash, block));
							while let Some((block_hash, block)) = blocks.pop_front() {
								// queue block for verification
								chain.remove_block(&block_hash);

								match self.verification_work_sender {
									Some(ref verification_work_sender) => {
										chain.verify_block_hash(block_hash.clone());
										verification_work_sender
											.send(VerificationTask::VerifyBlock(block))
											.expect("Verification thread have the same lifetime as `Synchronization`")
									},
									None => chain.insert_best_block(block)
										.expect("Error inserting to db."),
								}

								// process orphan blocks
								if let Entry::Occupied(entry) = self.orphaned_blocks.entry(block_hash) {
									let (_, orphaned) = entry.remove_entry();
									blocks.extend(orphaned);
								}
							}
						},
						BlockState::Requested | BlockState::Scheduled => {
							// remember peer as useful
							self.peers.insert(peer_index);
							// remember as orphan block
							self.orphaned_blocks
								.entry(block.block_header.previous_header_hash.clone())
								.or_insert(Vec::new())
								.push((block_hash, block))
						}
					}
				},
			}

			// requested block is received => move to saturated state if there are no more blocks
			chain.length_of_state(BlockState::Scheduled) == 0
				&& chain.length_of_state(BlockState::Requested) == 0
		};

		if switch_to_saturated {
			self.switch_to_saturated_state(true);
		}
	}

	/// Schedule new synchronization tasks, if any.
	fn execute_synchronization_tasks(&mut self) {
		let mut tasks: Vec<Task> = Vec::new();
		let idle_peers = self.peers.idle_peers();
		let idle_peers_len = idle_peers.len() as u32;

		// prepare synchronization tasks
		if idle_peers_len != 0 {
			// display information if processed many blocks || enough time has passed since sync start
			let mut chain = self.chain.write();
			if let State::Synchronizing(timestamp, num_of_blocks) = self.state {
				let new_timestamp = time::precise_time_s();
				let timestamp_diff = new_timestamp - timestamp;
				let new_num_of_blocks = chain.best_block().number;
				let blocks_diff = if new_num_of_blocks > num_of_blocks { new_num_of_blocks - num_of_blocks} else { 0 };
				if timestamp_diff >= 60.0 || blocks_diff > 1000 {
					self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);

					info!(target: "sync", "Processed {} blocks in {} seconds. Chain information: {:?}"
						, blocks_diff, timestamp_diff
						, chain.information());
				}
			}

			// check if we can query some blocks hashes
			let scheduled_hashes_len = chain.length_of_state(BlockState::Scheduled);
			if scheduled_hashes_len < MAX_SCHEDULED_HASHES {
				tasks.push(Task::RequestInventory(idle_peers[0]));
				self.peers.on_inventory_requested(idle_peers[0]);
			}

			// check if we can move some blocks from scheduled to requested queue
			let requested_hashes_len = chain.length_of_state(BlockState::Requested);
			let verifying_hashes_len = chain.length_of_state(BlockState::Verifying);
			if requested_hashes_len + verifying_hashes_len < MAX_REQUESTED_BLOCKS + MAX_VERIFYING_BLOCKS && scheduled_hashes_len != 0 {
				let chunk_size = min(MAX_BLOCKS_IN_REQUEST, max(scheduled_hashes_len / idle_peers_len, MIN_BLOCKS_IN_REQUEST));
				for idle_peer in idle_peers {
					let peer_chunk_size = min(chain.length_of_state(BlockState::Scheduled), chunk_size);
					if peer_chunk_size == 0 {
						break;
					}

					let requested_hashes = chain.request_blocks_hashes(peer_chunk_size);
					self.peers.on_blocks_requested(idle_peer, &requested_hashes);
					tasks.push(Task::RequestBlocks(idle_peer, requested_hashes));
				}
			}
		}

		// execute synchronization tasks
		for task in tasks {
			self.executor.lock().execute(task);
		}
	}

	/// Switch to saturated state
	fn switch_to_saturated_state(&mut self, ask_for_inventory: bool) {
		self.state = State::Saturated;
		self.orphaned_blocks.clear();
		self.peers.reset();

		{
			let mut chain = self.chain.write();
			chain.remove_blocks_with_state(BlockState::Requested);
			chain.remove_blocks_with_state(BlockState::Scheduled);
		}

		if ask_for_inventory {
			let mut executor = self.executor.lock();
			for idle_peer in self.peers.idle_peers() {
				self.peers.on_inventory_requested(idle_peer);
				executor.execute(Task::RequestInventory(idle_peer));
			}
		}
	}

	/// Thread procedure for handling verification tasks
	fn verification_worker_proc(sync: Arc<Mutex<Self>>, storage: Arc<db::Store>, work_receiver: Receiver<VerificationTask>) {
		let verifier = ChainVerifier::new(storage);
		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::VerifyBlock(block) => {
					match verifier.verify(&block) {
						Ok(_chain) => {
							sync.lock().on_block_verification_success(block)
						},
						Err(err) => {
							sync.lock().on_block_verification_error(&err, &block.hash())
						}
					}
				},
				_ => break,
			}
		}
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use parking_lot::{Mutex, RwLock};
	use tokio_core::reactor::{Core, Handle};
	use chain::{Block, RepresentH256};
	use super::{Client, Config, SynchronizationClient};
	use synchronization_executor::Task;
	use synchronization_chain::{Chain, ChainRef};
	use synchronization_executor::tests::DummyTaskExecutor;
	use p2p::event_loop;
	use test_data;
	use db;
	use primitives::hash::H256;

	fn create_sync() -> (Core, Handle, Arc<Mutex<DummyTaskExecutor>>, Arc<Mutex<SynchronizationClient<DummyTaskExecutor>>>) {
		let event_loop = event_loop();
		let handle = event_loop.handle();
		let storage = Arc::new(db::TestStorage::with_genesis_block());
		let chain = ChainRef::new(RwLock::new(Chain::new(storage.clone())));
		let executor = DummyTaskExecutor::new();
		let config = Config { threads_num: 1, skip_verification: true };
		let client = SynchronizationClient::new(config, &handle, executor.clone(), chain);
		(event_loop, handle, executor, client)
	} 

	#[test]
	fn synchronization_saturated_on_start() {
		let (_, _, _, sync) = create_sync();
		let sync = sync.lock();
		let info = sync.information();
		assert!(!info.state.is_synchronizing());
		assert_eq!(info.orphaned, 0);
	}

	#[test]
	fn synchronization_in_order_block_path() {
		let (_, _, executor, sync) = create_sync();

		let mut sync = sync.lock();
		let block1: Block = test_data::block_h1();
		let block2: Block = test_data::block_h2();

		sync.on_new_blocks_inventory(5, vec![block1.hash()]);
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 2);
		assert_eq!(tasks[0], Task::RequestInventory(5));
		assert_eq!(tasks[1], Task::RequestBlocks(5, vec![block1.hash()]));
		assert!(sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push unknown block => will be queued as orphan
		sync.on_peer_block(5, block2);
		assert!(sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 1);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push requested block => should be moved to the test storage && orphan should be moved
		sync.on_peer_block(5, block1);
		assert!(!sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 3);
		// we have just requested new `inventory` from the peer => peer is forgotten
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 0);
	}

	#[test]
	fn synchronization_out_of_order_block_path() {
		let (_, _, _, sync) = create_sync();
		let mut sync = sync.lock();

		let block2: Block = test_data::block_h169();

		sync.on_new_blocks_inventory(5, vec![block2.hash()]);
		sync.on_peer_block(5, block2);

		// out-of-order block was presented by the peer
		assert!(!sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 1);
		// we have just requested new `inventory` from the peer => peer is forgotten
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 0);
		// TODO: check that peer is penalized
	}

	#[test]
	fn synchronization_parallel_peers() {
		let (_, _, executor, sync) = create_sync();

		let block1: Block = test_data::block_h1();
		let block2: Block = test_data::block_h2();

		{
			let mut sync = sync.lock();
			// not synchronizing after start
			assert!(!sync.information().state.is_synchronizing());
			// receive inventory from new peer#1
			sync.on_new_blocks_inventory(1, vec![block1.hash()]);
			assert_eq!(sync.information().chain.requested, 1);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert!(sync.information().state.is_synchronizing());
			assert_eq!(tasks, vec![Task::RequestInventory(1), Task::RequestBlocks(1, vec![block1.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive inventory from new peer#2
			sync.on_new_blocks_inventory(2, vec![block1.hash(), block2.hash()]);
			assert_eq!(sync.information().chain.requested, 2);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert!(sync.information().state.is_synchronizing());
			assert_eq!(tasks, vec![Task::RequestInventory(2), Task::RequestBlocks(2, vec![block2.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive block from peer#2
			sync.on_peer_block(2, block2);
			assert!(sync.information().chain.requested == 1
				&& sync.information().orphaned == 1);
			// receive block from peer#1
			sync.on_peer_block(1, block1);
			assert!(sync.information().chain.requested == 0
				&& sync.information().orphaned == 0
				&& sync.information().chain.stored == 3);
		}
	}

	#[test]
	fn synchronization_reset_when_peer_is_disconnected() {
		let (_, _, _, sync) = create_sync();

		// request new blocks
		{
			let mut sync = sync.lock();
			sync.on_new_blocks_inventory(1, vec![H256::from(0)]);
			assert!(sync.information().state.is_synchronizing());
		}

		// lost connection to peer => synchronization state lost
		{
			let mut sync = sync.lock();
			sync.on_peer_disconnected(1);
			assert!(!sync.information().state.is_synchronizing());
		}
	}

	#[test]
	fn synchronization_not_starting_when_receiving_known_blocks() {
		let (_, _, executor, sync) = create_sync();
		let mut sync = sync.lock();
		// saturated => receive inventory with known blocks only
		sync.on_new_blocks_inventory(1, vec![test_data::genesis().hash()]);
		// => no need to start synchronization
		assert!(!sync.information().state.is_synchronizing());
		// => no synchronization tasks are scheduled
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks, vec![]);
	}

	#[test]
	fn synchronization_asks_for_inventory_after_saturating() {
		let (_, _, executor, sync) = create_sync();
		let mut sync = sync.lock();
		let block = test_data::block_h1();
		let block_hash = block.hash();
		sync.on_new_blocks_inventory(1, vec![block_hash.clone()]);
		sync.on_new_blocks_inventory(2, vec![block_hash.clone()]);
		executor.lock().take_tasks();
		sync.on_peer_block(2, block);

		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 2);
		assert!(tasks.iter().any(|t| t == &Task::RequestInventory(1)));
		assert!(tasks.iter().any(|t| t == &Task::RequestInventory(2)));
	}
}
