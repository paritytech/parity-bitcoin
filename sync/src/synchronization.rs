use std::thread;
use std::sync::Arc;
use std::cmp::{min, max};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
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
use time;

///! Blocks synchronization process:
///!
///! TODO: Current assumptions:
///! 1) unknown blocks in `inventory` messages are returned as a consequent range, sorted from oldest to newest
///! 2) no forks support
///!
///! When new peer is connected:
///! 1) send `inventory` message with full block locator hashes
///!
///! When `inventory` message is received from peer:
///! 1) if synchronization queue is empty:
///! 1.1) append all unknown blocks hashes to the `queued_hashes`
///! 1.2) mark peer as 'useful' for current synchronization stage (TODO)
///! 1.3) stop
///! 2) if intersection(`queued_hashes`, unknown blocks) is not empty && there are new unknown blocks:
///! 2.1) append new unknown blocks to the queued_hashes
///! 2.2) mark peer as 'useful' for current synchronization stage (TODO)
///! 2.3) stop
///! 3) if intersection(`queued_hashes`, unknown blocks) is not empty && there are no new unknown blocks:
///! 3.1) looks like peer is behind us in the blockchain (or these are blocks for the future)
///! 3.2) mark peer as 'suspicious' for current synchronization stage (TODO)
///! 3.3) stop
///!
///! After receiving `block` message:
///! 1) if any basic verification is failed (TODO):
///! 1.1) penalize peer
///! 1.2) stop
///! 1) if not(remove block) [i.e. block was not requested]:
///! 1.1) ignore it (TODO: try to append to the chain)
///! 1.2) stop
///! 2) if this block is first block in the `requested_hashes`:
///! 2.1) append to the verification queue (+ append to `verifying_hashes`) (TODO)
///! 2.2) for all children (from `orphaned_blocks`): append to the verification queue (TODO)
///! 2.3) stop
///! 3) remember in `orphaned_blocks`
///!
///! After receiving `inventory` message OR receiving `block` message:
///! 1) if there are blocks hashes in `queued_hashes`:
///! 1.1) select idle peers
///! 1.2) for each idle peer: query blocks from `queued_hashes`
///! 1.3) move requested blocks hashes from `queued_hashes` to `requested_hashes`
///! 1.4) mark idle peers as active
///! 2) if `queued_hashes` queue is not yet saturated:
///! 2.1) for each idle peer: send shortened `getblocks` message
///! 2.2) 'forget' idle peers (mark them as not useful for synchronization) (TODO)
///!
///! TODO: spawn management thread [watch for not-stalling sync]
///! TODO: check + optimize algorithm for Saturated state

/// Approximate maximal number of blocks hashes in scheduled queue.
const MAX_SCHEDULED_HASHES: u32 = 4 * 1024;
/// Approximate maximal number of blocks hashes in requested queue.
const MAX_REQUESTED_BLOCKS: u32 = 512;
/// Approximate maximal number of blocks in verifying queue.
const MAX_VERIFYING_BLOCKS: u32 = 512;
/// Minimum number of blocks to request from peer
const MIN_BLOCKS_IN_REQUEST: u32 = 32;
/// Maximum number of blocks to request from peer
const MAX_BLOCKS_IN_REQUEST: u32 = 512;

/// Thread-safe reference to the `Synchronization`
pub type SynchronizationRef<T> = Arc<Mutex<Synchronization<T>>>;

/// Synchronization task for the peer.
#[derive(Debug, Eq, PartialEq)]
pub enum Task {
	/// Request given blocks.
	RequestBlocks(usize, Vec<H256>),
	/// Request full inventory using block_locator_hashes.
	RequestInventory(usize),
	/// Request inventory using best block locator only.
	RequestBestInventory(usize),
}

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

/// Synchronization task executor
pub trait TaskExecutor {
	fn execute(&mut self, task: Task);
}

/// Verification thread tasks
enum VerificationTask {
	/// Verify single block
	VerifyBlock(Block),
	/// Stop verification thread
	Stop,
}

/// Synchronization config
#[derive(Default)]
pub struct Config {
	/// Skip blocks verification
	pub skip_block_verification: bool,
} 

/// New blocks synchronization process.
pub struct Synchronization<T: TaskExecutor + Send + 'static> {
	/// Synchronization state.
	state: State,
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

impl State {
	pub fn is_synchronizing(&self) -> bool {
		match self {
			&State::Synchronizing(_, _) => true,
			_ => false,
		}
	}
}

impl<T> Drop for Synchronization<T> where T: TaskExecutor + Send + 'static {
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

impl<T> Synchronization<T> where T: TaskExecutor + Send + 'static {
	/// Create new synchronization window
	pub fn new(config: Config, executor: Arc<Mutex<T>>, chain: ChainRef) -> SynchronizationRef<T> {
		let sync = SynchronizationRef::new(Mutex::new(
			Synchronization {
				state: State::Saturated,
				peers: Peers::new(),
				executor: executor,
				chain: chain.clone(),
				orphaned_blocks: HashMap::new(),
				verification_work_sender: None,
				verification_worker_thread: None,
			}
		));

		if !config.skip_block_verification {
			let (verification_work_sender, verification_work_receiver) = channel();
			let csync = sync.clone();
			let mut lsync = sync.lock();
			let storage = chain.read().storage();
			lsync.verification_work_sender = Some(verification_work_sender);
			lsync.verification_worker_thread = Some(thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					Synchronization::verification_worker_proc(csync, storage, verification_work_receiver)
				})
				.expect("Error creating verification thread"));
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

	/// Try to queue synchronization of unknown blocks when new inventory is received.
	pub fn on_new_blocks_inventory(&mut self, peer_index: usize, peer_hashes: Vec<H256>) {
		self.process_new_blocks_inventory(peer_index, peer_hashes);
		self.execute_synchronization_tasks();
	}

	/// Process new block.
	pub fn on_peer_block(&mut self, peer_index: usize, block: Block) {
		let block_hash = block.hash();

		// update peers to select next tasks
		self.peers.on_block_received(peer_index, &block_hash);

		self.process_peer_block(block_hash, block);
		self.execute_synchronization_tasks();
	}

	/// Reset synchronization process
	pub fn reset(&mut self) {
		self.peers.reset();
		self.orphaned_blocks.clear();
		// TODO: reset verification queue

		let mut chain = self.chain.write();
		self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);
		chain.remove_blocks_with_state(BlockState::Requested);
		chain.remove_blocks_with_state(BlockState::Scheduled);
		chain.remove_blocks_with_state(BlockState::Verifying);

		warn!(target: "sync", "Synchronization process restarting from block {:?}", chain.best_block());
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

		// new block is scheduled => move to synchronizing state
		if !self.state.is_synchronizing() {
			self.state = State::Synchronizing(time::precise_time_s(), chain.best_block().number);
		}

		// when synchronization is idling
		// => request full inventory
		if !chain.has_blocks_of_state(BlockState::Scheduled)
			&& !chain.has_blocks_of_state(BlockState::Requested) {
			chain.schedule_blocks_hashes(peer_hashes);
			self.peers.insert(peer_index);
			return;
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
			if last_known_peer_hash_index == 0 {
				// either these are blocks from the future or blocks from the past
				// => TODO: ignore this peer during synchronization
				return;
			}

			if chain.block_has_state(&peer_hashes[last_known_peer_hash_index], BlockState::Scheduled) {
				// we have found first block which is scheduled
				// => blocks in range [(last_known_peer_hash_index + 1)..peer_hashes_len] are unknown
				let unknown_peer_hashes = peer_hashes.split_off(last_known_peer_hash_index + 1);
				chain.schedule_blocks_hashes(unknown_peer_hashes);
				self.peers.insert(peer_index);
				return;
			}

			last_known_peer_hash_index -= 1;
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
					let (orphaned_parent_hash, orphaned_block) = orphaned_block_entry.remove_entry();
					current_block_hash = orphaned_parent_hash;
					current_block = orphaned_block;
				}
				else {
					break;
				}
			}

			return;
		}

		// this block is not the next one => mark it as orphaned
		self.orphaned_blocks.insert(block_hash, block);
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
					self.state = State::Synchronizing(new_timestamp, new_num_of_blocks);

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
	fn verification_worker_proc(sync: SynchronizationRef<T>, storage: Arc<db::Store>, work_receiver: Receiver<VerificationTask>) {
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
		self.reset();

		// start new tasks
		self.execute_synchronization_tasks();
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use std::mem::replace;
	use parking_lot::{Mutex, RwLock};
	use chain::{Block, RepresentH256};
	use super::{Synchronization, SynchronizationRef, Config, Task, TaskExecutor};
	use synchronization_chain::{Chain, ChainRef};
	use db;

	#[derive(Default)]
	struct DummyTaskExecutor {
		pub tasks: Vec<Task>,
	}

	impl DummyTaskExecutor {
		pub fn take_tasks(&mut self) -> Vec<Task> {
			replace(&mut self.tasks, Vec::new())
		}
	}

	impl TaskExecutor for DummyTaskExecutor {
		fn execute(&mut self, task: Task) {
			self.tasks.push(task);
		}
	}

	fn create_sync() -> (Arc<Mutex<DummyTaskExecutor>>, SynchronizationRef<DummyTaskExecutor>) {
		let storage = Arc::new(db::TestStorage::with_genesis_block());
		let chain = ChainRef::new(RwLock::new(Chain::new(storage.clone())));
		let executor = Arc::new(Mutex::new(DummyTaskExecutor::default()));
		(executor.clone(), Synchronization::new(Config {
			skip_block_verification: true,
		}, executor, chain))
	} 

	#[test]
	fn synchronization_saturated_on_start() {
		let (_, sync) = create_sync();
		let sync = sync.lock();
		let info = sync.information();
		assert!(!info.state.is_synchronizing());
		assert_eq!(info.orphaned, 0);
	}

	#[test]
	fn synchronization_in_order_block_path() {
		let (executor, sync) = create_sync();

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
		let (_, sync) = create_sync();
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
}
