use std::cmp::{min, max};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use chain::{Block, RepresentH256};
use primitives::hash::H256;
use hash_queue::HashPosition;
use synchronization_peers::{Peers, Information as PeersInformation};
use synchronization_chain::{ChainRef, Information as ChainInformation, BlockState};

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


/// Approximate maximal number of blocks hashes in queued_hashes_set.
const MAX_SCHEDULED_HASHES: u64 = 4 * 1024;
/// Approximate maximal number of blocks hashes in requested_hashes_set.
const MAX_REQUESTED_BLOCKS: u64 = 512;
/// Minimum number of blocks to request from peer
const MIN_BLOCKS_IN_REQUEST: u64 = 32;
/// Maximum number of blocks to request from peer
const MAX_BLOCKS_IN_REQUEST: u64 = 512;

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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum State {
	Synchronizing,
	Saturated,
}

/// Information on current synchronization state.
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

/// New blocks synchronization process.
pub struct Synchronization {
	/// Synchronization state.
	state: State,
	/// Synchronization peers.
	peers: Peers,
	/// Chain reference.
	chain: ChainRef,
	/// Blocks from requested_hashes, but received out-of-order.
	orphaned_blocks: HashMap<H256, Block>,
}

impl Synchronization {
	/// Create new synchronization window
	pub fn new(chain: ChainRef) -> Synchronization {
		Synchronization {
			state: State::Saturated,
			peers: Peers::new(),
			chain: chain,
			orphaned_blocks: HashMap::new(),
		}
	}

	/// Get information on current synchronization state.
	pub fn information(&self) -> Information {
		Information {
			state: self.state,
			peers: self.peers.information(),
			chain: self.chain.read().information(),
			orphaned: self.orphaned_blocks.len(),
		}
	}

	/// Try to queue synchronization of unknown blocks when new inventory is received.
	pub fn on_unknown_blocks(&mut self, peer_index: usize, mut peer_hashes: Vec<H256>) {
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

		// new block is requested => move to synchronizing state
		self.state = State::Synchronizing;

		// when synchronization is idling
		// => request full inventory
		let mut chain = self.chain.write();
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

	/// Process new block.
	pub fn on_peer_block(&mut self, peer_index: usize, block: Block) {
		let block_hash = block.hash();

		// update peers to select next tasks
		self.peers.on_block_received(peer_index, &block_hash);

		// this block is not requested for synchronization
		let mut chain = self.chain.write();
		let block_position = chain.remove_block_with_state(&block_hash, BlockState::Requested);
		if block_position == HashPosition::Missing {
			return;
		}

		// requeste block is received => move to saturated state if there are no more blocks
		if !chain.has_blocks_of_state(BlockState::Scheduled)
			&& !chain.has_blocks_of_state(BlockState::Requested) {
			self.state = State::Saturated;
		}

		// check if this block is next block in the blockchain
		if block_position == HashPosition::Front {
			// this is next block in the blockchain => queue for verification
			chain.verify_and_insert_block(block_hash.clone(), block);

			// check orphaned blocks
			let mut orphaned_parent_hash = block_hash;
			while let Entry::Occupied(orphaned_block_entry) = self.orphaned_blocks.entry(orphaned_parent_hash) {
				let (_, orphaned_block) = orphaned_block_entry.remove_entry();
				orphaned_parent_hash = orphaned_block.hash();
				chain.verify_and_insert_block(orphaned_parent_hash.clone(), orphaned_block);
			}

			return;
		}

		// this block is not the next one => mark it as orphaned
		self.orphaned_blocks.insert(block_hash, block);
	}

	/// Schedule new synchronization tasks, if any.
	pub fn get_synchronization_tasks(&mut self) -> Vec<Task> {
		let mut tasks: Vec<Task> = Vec::new();

		// check if we can query some blocks hashes
		let mut chain = self.chain.write();
		let scheduled_hashes_len = chain.length_of_state(BlockState::Scheduled);
		if scheduled_hashes_len < MAX_SCHEDULED_HASHES {
			if self.state == State::Synchronizing {
				if let Some(idle_peer) = self.peers.idle_peer() {
					tasks.push(Task::RequestBestInventory(idle_peer));
					self.peers.on_inventory_requested(idle_peer);
				}
			}
			else {
				if let Some(idle_peer) = self.peers.idle_peer() {
					tasks.push(Task::RequestInventory(idle_peer));
					self.peers.on_inventory_requested(idle_peer);
				}
			}
		}

		// check if we can move some blocks from scheduled to requested queue
		let requested_hashes_len = chain.length_of_state(BlockState::Requested);
		if requested_hashes_len < MAX_REQUESTED_BLOCKS && scheduled_hashes_len != 0 {
			let idle_peers = self.peers.idle_peers();
			let idle_peers_len = idle_peers.len() as u64;
			if idle_peers_len != 0 {
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

		tasks
	}

	/// Calculate block locator hashes for given store
	fn block_locator_hashes_for(local_index: usize, mut step: usize, store: &VecDeque<H256>, target: &mut Vec<H256>) -> (usize, usize) {
		let store_len = store.len();
		if store_len == 0 {
			return (local_index, step);
		}
		if store.len() - 1 < local_index {
			return (local_index - store.len() - 1, step);
		}

		let mut local_index = store.len() - 1 - local_index;
		loop {
			let hash = store[local_index].clone();
			target.push(hash);

			if target.len() >= 10 {
				step <<= 1;
			}
			if local_index < step {
				return (step - local_index - 1, step);
			}
			local_index -= step;
		}
	}

	/// Add block to the verification queue.
	fn queue_block_for_verification(chain: &mut LocalChain, peers: &mut Peers, peer_index: Option<usize>, block: Block) {
		// TODO: add another basic verifications here (use verification package)
		// TODO: return error if basic verification failed && reset synchronization state
		if chain.best_block().hash != block.block_header.previous_header_hash {
			// penalize peer
			if let Some(peer_index) = peer_index {
				peers.on_wrong_block_received(peer_index);
			}
			return;
		}

		// TODO: move to the verification queue instead of local_chain
		chain.insert_block(block);
	}

	#[cfg(test)]
	pub fn peers(&'a self) -> &'a Peers {
		&self.peers
	}

	#[cfg(test)]
	pub fn requested_hashes_mut(&'a mut self) -> &'a mut VecDeque<H256> {
		&mut self.requested_hashes
	}

	#[cfg(test)]
	pub fn queued_hashes_mut(&'a mut self) -> &'a mut VecDeque<H256> {
		&mut self.queued_hashes
	}

	#[cfg(test)]
	pub fn verifying_hashes_mut(&'a mut self) -> &'a mut VecDeque<H256> {
		&mut self.verifying_hashes
	}	
}

#[cfg(test)]
mod tests {
	use chain::{Block, RepresentH256};
	use primitives::hash::H256;
	use local_chain::LocalChain;
	use super::{Synchronization, State, Task};

	#[test]
	fn synchronization_saturated_on_start() {
		let sync = Synchronization::new();
		let info = sync.information();
		assert_eq!(info.state, State::Saturated);
		assert_eq!(info.requested, 0);
		assert_eq!(info.queued, 0);
		assert_eq!(info.orphaned, 0);
	}

	#[test]
	fn synchronization_in_order_block_path() {
		let mut chain = LocalChain::new();
		let mut sync = Synchronization::new();

		let block1: Block = "010000006fe28c0ab6f1b372c1a6a246ae63f74f931e8365e15a089c68d6190000000000982051fd1e4ba744bbbe680e1fee14677ba1a3c3540bf7b1cdb606e857233e0e61bc6649ffff001d01e362990101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d0104ffffffff0100f2052a0100000043410496b538e853519c726a2c91e61ec11600ae1390813a627c66fb8be7947be63c52da7589379515d4e0a604f8141781e62294721166bf621e73a82cbf2342c858eeac00000000".into();
		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();

		sync.on_unknown_blocks(5, vec![block1.hash()]);
		assert_eq!(sync.information().state, State::Synchronizing);
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 1);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 1);
		assert_eq!(sync.information().peers.active, 0);

		let tasks = sync.get_synchronization_tasks();
		assert_eq!(tasks.len(), 2);
		assert_eq!(tasks[0], Task::RequestBestInventory(5));
		assert_eq!(tasks[1], Task::RequestBlocks(5, vec![block1.hash()]));
		assert_eq!(sync.information().state, State::Synchronizing);
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push unknown block => nothing should change
		sync.on_peer_block(5, block2);
		assert_eq!(sync.information().state, State::Synchronizing);
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 1);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 0);
		assert_eq!(sync.information().peers.active, 1);

		// push requested block => nothing should change
		sync.on_peer_block(5, block1);
		assert_eq!(sync.information().state, State::Saturated);
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 2);
		assert_eq!(sync.information().peers.idle, 1);
		assert_eq!(sync.information().peers.active, 0);
	}

	#[test]
	fn synchronization_out_of_order_block_path() {
		let chain = ChainRef::new(RwLock::new(Chain::with_test_storage()));
		let mut sync = Synchronization::new(chain);

		let block2: Block = "010000004860eb18bf1b1620e37e9490fc8a427514416fd75159ab86688e9a8300000000d5fdcc541e25de1c7a5addedf24858b8bb665c9f36ef744ee42c316022c90f9bb0bc6649ffff001d08d2bd610101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704ffff001d010bffffffff0100f2052a010000004341047211a824f55b505228e4c3d5194c1fcfaa15a456abdf37f9b9d97a4040afc073dee6c89064984f03385237d92167c13e236446b417ab79a0fcae412ae3316b77ac00000000".into();

		sync.on_unknown_blocks(5, vec![block2.hash()]);
		sync.get_synchronization_tasks();
		sync.on_peer_block(5, block2);

		// out-of-order block was presented by the peer
		assert_eq!(sync.information().state, State::Saturated);
		assert_eq!(sync.information().orphaned, 0);
		assert_eq!(sync.information().chain.scheduled, 0);
		assert_eq!(sync.information().chain.requested, 0);
		assert_eq!(sync.information().chain.stored, 1);
		assert_eq!(sync.information().peers.idle, 1);
		assert_eq!(sync.information().peers.active, 0);
		// TODO: check that peer is penalized
	}
}
