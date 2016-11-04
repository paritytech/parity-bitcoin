use std::thread;
use std::sync::Arc;
use std::cmp::{min, max};
use std::collections::HashMap;
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
use hash_queue::HashPosition;
use synchronization_peers::Peers;
#[cfg(test)] use synchronization_peers::{Information as PeersInformation};
use synchronization_chain::{ChainRef, BlockState};
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
///! 2.4.3.1) scheduled_blocks.append(inventory_rest)
///! 2.4.3.2) stop (2.4.3)
///! 2.4.4) if !queue_rest.is_empty(): ===> has new unknown blocks in inventory, fork
///! 2.4.4.1) scheduled_blocks.append(inventory_rest)
///! 2.4.4.2) stop (2.4.4)
///! 2.4.5) stop (2.4)
///! 2.5) stop (2)
///! 3) if queue_intersection.is_empty(): ===> responded with out-of-sync-window blocks
///! 3.1) last_known_block = inventory.last(b => b.is_known())
///! 3.2) if last_known_block == None: ===> we know nothing about these blocks & we haven't asked for these
///! 3.2.1) peer will be excluded later by management thread
///! 3.2.2) stop (3.2)
///! 3.3) if last_known_block == last(inventory): ===> responded with all-known-blocks
///! 3.3.1) remember peer as useful (possibly had failures before && have been excluded from sync)
///! 3.3.2) stop (3.3)
///! 3.4) if last_known_block in the middle of inventory: ===> responded with forked blocks
///! 3.4.1) remember peer as useful
///! 3.4.2) inventory_rest = inventory after last_known_block
///! 3.4.3) scheduled_blocks.append(inventory_rest)
///! 3.4.4) stop (3.4)
///! 3.5) stop (3)
///!
///! on_peer_block: After receiving `block` message:
///! 1) if block_state(block) in (Scheduled, Verifying, Stored): ===> late delivery
///! 1.1) remember peer as useful
///! 1.2) stop (1)
///! 2) if block_state(block) == Requested: ===> on-time delivery
///! 2.1) remember peer as useful
///! 2.2) move block from requested to verifying queue
///! 2.2) queue verification().and_then(insert).or_else(reset_sync)
///! 2.3) stop (2)
///! 3) if block_state(block) == Unknown: ===> maybe we are on-top of chain && new block is announced?
///! 3.1) if block_state(block.parent_hash) == Unknown: ===> we do not know parent
///! 3.1.1) ignore this block
///! 3.1.2) stop (3.1)
///! 3.2) if block_state(block.parent_hash) != Unknown: ===> fork found
///! 3.2.1) ask peer for best inventory (after this block)
///! 3.2.2) append block to verifying queue
///! 3.2.3) queue verification().and_then(insert).or_else(reset_sync)
///! 3.2.4) stop (3.2)
///! 2.3) stop (2)
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
///! 2) remove all known children from all queues [so that new `block` messages will be ignored in on_peer_block.3.1.1]
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
	fn reset(&mut self, is_hard: bool);
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
	orphaned_blocks: HashMap<H256, Block>,
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
			self.verification_work_sender
				.take()
				.expect("Some(join_handle) => Some(verification_work_sender)")
				.send(VerificationTask::Stop).expect("TODO");
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

		self.process_peer_block(block_hash, block);
		self.execute_synchronization_tasks();
	}

	/// Peer disconnected.
	fn on_peer_disconnected(&mut self, peer_index: usize) {
		self.peers.on_peer_disconnected(peer_index);

		// when last peer is disconnected, reset, but let verifying blocks be verified
		self.reset(false);
	}

	/// Reset synchronization process
	fn reset(&mut self, is_hard: bool) {
		self.peers.reset();
		self.orphaned_blocks.clear();
		// TODO: reset verification queue

		let mut chain = self.chain.write();
		chain.remove_blocks_with_state(BlockState::Requested);
		chain.remove_blocks_with_state(BlockState::Scheduled);
		if is_hard {
			self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);
			chain.remove_blocks_with_state(BlockState::Verifying);
			warn!(target: "sync", "Synchronization process restarting from block {:?}", chain.best_block());
		}
		else {
			self.state = State::Saturated;
		}
	}

	/// Process successful block verification
	fn on_block_verification_success(&mut self, block: Block) {
		{
			let hash = block.hash();
			let mut chain = self.chain.write();

			// remove from verifying queue
			assert_eq!(chain.remove_block_with_state(&hash, BlockState::Verifying), HashPosition::Front);

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

		// reset synchronization process
		self.reset(true);

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
					manage_synchronization_peers(&mut client.peers);
					client.execute_synchronization_tasks();
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
	fn process_new_blocks_inventory(&mut self, peer_index: usize, mut peer_hashes: Vec<H256>) {
		//     | requested | QUEUED |
		// ---                          [1]
		//         ---                  [2] +
		//                   ---        [3] +
		//                          --- [4]
		//    -+-                       [5] +
		//              -+-             [6] +
		//                       -+-    [7] +
		//  ---+---------+---           [8] +
		//            ---+--------+---  [9] +
		//  ---+---------+--------+---  [10]

		let mut chain = self.chain.write();

		'outer: loop {
			// when synchronization is idling
			// => request full inventory
			if !chain.has_blocks_of_state(BlockState::Scheduled)
				&& !chain.has_blocks_of_state(BlockState::Requested) {
				let unknown_blocks: Vec<_> = peer_hashes.into_iter()
					.filter(|hash| chain.block_has_state(&hash, BlockState::Unknown))
					.collect();

				// no new blocks => no need to switch to the synchronizing state
				if unknown_blocks.is_empty() {
					return;
				}

				chain.schedule_blocks_hashes(unknown_blocks);
				self.peers.insert(peer_index);
				break;
			}

			// cases: [2], [5], [6], [8]
			// if last block from peer_hashes is in window { requested_hashes + queued_hashes }
			// => no new blocks for synchronization, but we will use this peer in synchronization
			let peer_hashes_len = peer_hashes.len();
			if chain.block_has_state(&peer_hashes[peer_hashes_len - 1], BlockState::Scheduled)
				|| chain.block_has_state(&peer_hashes[peer_hashes_len - 1], BlockState::Requested) {
				self.peers.insert(peer_index);
				return;
			}

			// cases: [1], [3], [4], [7], [9], [10]
			// try to find new blocks for synchronization from inventory
			let mut last_known_peer_hash_index = peer_hashes_len - 1;
			loop {
				if chain.block_state(&peer_hashes[last_known_peer_hash_index]) != BlockState::Unknown {
					// we have found first block which is known to us
					// => blocks in range [(last_known_peer_hash_index + 1)..peer_hashes_len] are unknown
					//    && must be scheduled for request
					let unknown_peer_hashes = peer_hashes.split_off(last_known_peer_hash_index + 1);

					chain.schedule_blocks_hashes(unknown_peer_hashes);
					self.peers.insert(peer_index);
					break 'outer;
				}

				if last_known_peer_hash_index == 0 {
					// either these are blocks from the future or blocks from the past
					// => TODO: ignore this peer during synchronization
					return;
				}
				last_known_peer_hash_index -= 1;
			}
		}

		// move to synchronizing state
		if !self.state.is_synchronizing() {
			self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);
		}
	}

	/// Process new peer block
	fn process_peer_block(&mut self, block_hash: H256, block: Block) {
		// this block is not requested for synchronization
		let mut chain = self.chain.write();
		let block_position = chain.remove_block_with_state(&block_hash, BlockState::Requested);
		if block_position == HashPosition::Missing {
			return;
		}

		// requested block is received => move to saturated state if there are no more blocks
		if !chain.has_blocks_of_state(BlockState::Scheduled)
			&& !chain.has_blocks_of_state(BlockState::Requested)
			&& !chain.has_blocks_of_state(BlockState::Verifying) {
			self.state = State::Saturated;
		}

		// check if this block is next block in the blockchain
		if block_position == HashPosition::Front {
			// check if this parent of this block is current best_block
			let expecting_previous_header_hash = chain.best_block_of_state(BlockState::Verifying)
				.unwrap_or_else(|| {
					chain.best_block_of_state(BlockState::Stored)
						.expect("storage with genesis block is required")
				}).hash;
			if block.block_header.previous_header_hash != expecting_previous_header_hash {
				// TODO: penalize peer
				warn!(target: "sync", "Out-of-order block {:?} was dropped. Expecting block with parent hash {:?}", block_hash, expecting_previous_header_hash);
				return;
			}

			// this is next block in the blockchain => queue for verification
			// also unwrap all dependent orphan blocks
			let mut current_block = block;
			let mut current_block_hash = block_hash;
			loop {
				match self.verification_work_sender {
					Some(ref verification_work_sender) => {
						chain.verify_block_hash(current_block_hash.clone());
						verification_work_sender
							.send(VerificationTask::VerifyBlock(current_block))
							.expect("Verification thread have the same lifetime as `Synchronization`");
					},
					None => {
						chain.insert_best_block(current_block)
							.expect("Error inserting to db.");
					}
				};

				// proceed to the next orphaned block
				if let Entry::Occupied(orphaned_block_entry) = self.orphaned_blocks.entry(current_block_hash) {
					let (_, orphaned_block) = orphaned_block_entry.remove_entry();
					current_block = orphaned_block;
					current_block_hash = current_block.hash();
				}
				else {
					break;
				}
			}

			return;
		}

		// this block is not the next one => mark it as orphaned
		self.orphaned_blocks.insert(block.block_header.previous_header_hash.clone(), block);
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

			// TODO: instead of issuing duplicated inventory requests, wait until enough new blocks are verified, then issue
			// check if we can query some blocks hashes
			let scheduled_hashes_len = chain.length_of_state(BlockState::Scheduled);
			if scheduled_hashes_len < MAX_SCHEDULED_HASHES {
				if self.state.is_synchronizing() {
					tasks.push(Task::RequestBestInventory(idle_peers[0]));
					self.peers.on_inventory_requested(idle_peers[0]);
				}
				else {
					tasks.push(Task::RequestInventory(idle_peers[0]));
					self.peers.on_inventory_requested(idle_peers[0]);
				}
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
		let block1: Block = "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e362990101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000".into();
		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();

		sync.on_new_blocks_inventory(5, vec![block1.hash()]);
		let tasks = executor.lock().take_tasks();
		assert_eq!(tasks.len(), 2);
		assert_eq!(tasks[0], Task::RequestBestInventory(5));
		assert_eq!(tasks[1], Task::RequestBlocks(5, vec![block1.hash()]));
		assert!(sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push unknown block => nothing should change
		sync.on_peer_block(5, block2);
		assert!(sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push requested block => should be moved to the test storage
		sync.on_peer_block(5, block1);
		assert!(!sync.information().state.is_synchronizing());
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 2);
		// we have just requested new `inventory` from the peer => peer is forgotten
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 0);
	}

	#[test]
	fn synchronization_out_of_order_block_path() {
		let (_, _, _, sync) = create_sync();
		let mut sync = sync.lock();

		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();

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

		let block1: Block = "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e362990101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000".into();
		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();

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
			assert_eq!(tasks, vec![Task::RequestBestInventory(1), Task::RequestBlocks(1, vec![block1.hash()])]);
		}

		{
			let mut sync = sync.lock();
			// receive inventory from new peer#2
			sync.on_new_blocks_inventory(2, vec![block1.hash(), block2.hash()]);
			assert_eq!(sync.information().chain.requested, 2);
			// synchronization has started && new blocks have been requested
			let tasks = executor.lock().take_tasks();
			assert!(sync.information().state.is_synchronizing());
			assert_eq!(tasks, vec![Task::RequestBestInventory(2), Task::RequestBlocks(2, vec![block2.hash()])]);
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
			sync.on_new_blocks_inventory(1, vec!["0000000000000000000000000000000000000000000000000000000000000000".into()]);
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
}
