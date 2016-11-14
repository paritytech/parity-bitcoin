use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use primitives::hash::H256;
use linked_hash_map::LinkedHashMap;
use time::precise_time_s;

/// Max peer failures # before excluding from sync process
const MAX_PEER_FAILURES: usize = 8;

/// Set of peers selected for synchronization.
#[derive(Debug)]
pub struct Peers {
	/// Peers that are marked as useful for current synchronization session && have no pending requests.
	idle: HashSet<usize>,
	/// Peers that are marked as non-useful for current synchronization session && have no pending requests.
	unuseful: HashSet<usize>,
	/// Peers that are marked as useful for current synchronization session && have pending requests.
	active: HashMap<usize, HashSet<H256>>,
	/// # of failures for given peer.
	active_failures: HashMap<usize, usize>,
	/// Last message time from peer.
	active_order: LinkedHashMap<usize, f64>,
}

/// Information on synchronization peers
#[cfg(test)]
#[derive(Debug)]
pub struct Information {
	/// # of peers that are marked as useful for current synchronization session && have no pending requests.
	pub idle: usize,
	/// # of peers that are marked as non-useful for current synchronization session && have no pending requests.
	pub unuseful: usize,
	/// # of peers that are marked as useful for current synchronization session && have pending requests.
	pub active: usize,
}

impl Peers {
	pub fn new() -> Peers {
		Peers {
			idle: HashSet::new(),
			unuseful: HashSet::new(),
			active: HashMap::new(),
			active_failures: HashMap::new(),
			active_order: LinkedHashMap::new(),
		}
	}

	/// Get information on synchronization peers
	#[cfg(test)]
	pub fn information(&self) -> Information {
		Information {
			idle: self.idle.len(),
			unuseful: self.unuseful.len(),
			active: self.active.len(),
		}
	}

	/// Has any peers?
	pub fn any(&self) -> bool {
		!self.idle.is_empty()
			|| !self.unuseful.is_empty()
			|| !self.active.is_empty()
	}

	/// Get all peers
	pub fn all_peers(&self) -> Vec<usize> {
		self.idle_peers().into_iter()
			.chain(self.unuseful_peers().into_iter())
			.chain(self.active_peers().into_iter())
			.collect()
	}

	/// Get idle peers.
	pub fn idle_peers(&self) -> Vec<usize> {
		self.idle.iter().cloned().collect()
	}

	/// Get unuseful peers.
	pub fn unuseful_peers(&self) -> Vec<usize> {
		self.unuseful.iter().cloned().collect()
	}

	/// Get active peers.
	pub fn active_peers(&self) -> Vec<usize> {
		self.active.keys().cloned().collect()
	}

	/// Get active peers, sorted by last response time (oldest first).
	pub fn active_peers_order(&self) -> Vec<(usize, f64)> {
		self.active_order.iter()
			.map(|(&pi, &t)| (pi, t))
			.collect()
	}

	/// Insert new synchronization peer.
	pub fn insert(&mut self, peer_index: usize) {
		// if peer is unknown => insert to idle queue
		// if peer is known && not useful => insert to idle queue
		if !self.idle.contains(&peer_index)
			&& !self.active.contains_key(&peer_index) {
			self.idle.insert(peer_index);
			self.unuseful.remove(&peer_index);
		}
	}

	/// Peer has been disconnected
	pub fn on_peer_disconnected(&mut self, peer_index: usize) {
		// foret this peer without any chances to reuse
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.active.remove(&peer_index);
		self.active_failures.remove(&peer_index);
		self.active_order.remove(&peer_index);
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: usize, block_hash: &H256) {
		// got new message => forget all previous failures
		self.on_peer_message(peer_index);
		// if this is message from active peer && there are no pendind requests to this peer - mark as idle
		if let Entry::Occupied(mut active_entry) = self.active.entry(peer_index) {
			active_entry.get_mut().remove(block_hash);
			if active_entry.get().is_empty() {
				self.active_failures.remove(&peer_index);
				self.active_order.remove(&peer_index);
				self.idle.insert(peer_index);
				active_entry.remove_entry();
			}
		}
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: usize, blocks_hashes: &[H256]) {
		// mark peer as active
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.active.entry(peer_index).or_insert_with(HashSet::new).extend(blocks_hashes.iter().cloned());
		self.active_order.insert(peer_index, precise_time_s());
	}

	/// Inventory has been requested from peer.
	pub fn on_inventory_requested(&mut self, peer_index: usize) {
		// inventory can only be requested from idle peers
		assert!(!self.active.contains_key(&peer_index));

		// remove peer from idle && active queues & leave him as unuseful (will becaume useful again once we will receive `good` inventory from him)
		self.idle.remove(&peer_index);
		self.active.remove(&peer_index);
		self.active_failures.remove(&peer_index);
		self.active_order.remove(&peer_index);
		self.unuseful.insert(peer_index);
		// peer is now out-of-synchronization process, because:
		// 1) if it has new blocks, it will respond with `inventory` message && will be inserted back here
		// 2) if it has no new blocks => either synchronization is completed, or it is behind us in sync
	}

	/// We have failed to get response from peer during given period
	pub fn on_peer_failure(&mut self, peer_index: usize) -> bool {
		let peer_failures = match self.active_failures.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				let failures = entry.get() + 1;
				entry.insert(failures);
				failures
			},
			Entry::Vacant(entry) => *entry.insert(1),
		};

		let too_much_failures = peer_failures >= MAX_PEER_FAILURES;
		if too_much_failures {
			self.idle.remove(&peer_index);
			self.active.remove(&peer_index);
			self.active_failures.remove(&peer_index);
			self.active_order.remove(&peer_index);
			self.unuseful.insert(peer_index);
		}
		too_much_failures
	}

	/// Reset all peers state to the unuseful
	pub fn reset(&mut self) {
		self.unuseful.extend(self.idle.drain());
		self.unuseful.extend(self.active.drain().map(|(k, _)| k));
		self.active_failures.clear();
		self.active_order.clear();
	}

	/// Reset peer tasks && move peer to idle state
	pub fn reset_tasks(&mut self, peer_index: usize) -> Vec<H256> {
		let requests = self.active.remove(&peer_index);
		self.active_order.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.idle.insert(peer_index);
		requests.expect("empty requests queue is not allowed").into_iter().collect()
	}

	/// When sync message is received from peer
	fn on_peer_message(&mut self, peer_index: usize) {
		if self.active.contains_key(&peer_index) {
			self.active_failures.remove(&peer_index);
			self.active_order.remove(&peer_index);
			self.active_order.insert(peer_index, precise_time_s());
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{Peers, MAX_PEER_FAILURES};
	use primitives::hash::H256;

	#[test]
	fn peers_empty_on_start() {
		let peers = Peers::new();
		assert_eq!(peers.idle_peers(), vec![]);

		let info = peers.information();
		assert_eq!(info.idle, 0);
		assert_eq!(info.active, 0);
	}

	#[test]
	fn peers_all_unuseful_after_reset() {
		let mut peers = Peers::new();
		peers.on_blocks_requested(7, &vec![H256::default()]);
		peers.on_blocks_requested(8, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 2);
		peers.reset();
		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 2);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peer_idle_after_reset_tasks() {
		let mut peers = Peers::new();
		peers.on_blocks_requested(7, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);
		assert_eq!(peers.reset_tasks(7), vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_unuseful_after_inventory_request() {
		let mut peers = Peers::new();
		peers.insert(5);
		peers.insert(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		peers.on_inventory_requested(5);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 1);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_insert_remove_idle() {
		let mut peers = Peers::new();

		peers.insert(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers(), vec![0]);

		peers.insert(5);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		assert!(peers.idle_peers()[0] == 0 || peers.idle_peers()[0] == 5);
		assert!(peers.idle_peers()[1] == 0 || peers.idle_peers()[1] == 5);

		peers.on_peer_disconnected(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		assert!(peers.idle_peers()[0] == 0 || peers.idle_peers()[0] == 5);
		assert!(peers.idle_peers()[1] == 0 || peers.idle_peers()[1] == 5);

		peers.on_peer_disconnected(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers(), vec![5]);
	}

	#[test]
	fn peers_request_blocks() {
		let mut peers = Peers::new();

		peers.insert(5);

		peers.on_blocks_requested(7, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		peers.on_blocks_requested(8, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 2);

		peers.on_block_received(7, &H256::default());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(9, &H256::default());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(8, &"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(8, &H256::default());
		assert_eq!(peers.information().idle, 3);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_worst() {
		let mut peers = Peers::new();

		peers.insert(1);
		peers.insert(2);
		assert_eq!(peers.active_peers_order(), vec![]);

		peers.on_blocks_requested(1, &vec![H256::default()]);
		assert_eq!(peers.active_peers_order().len(), 1);
		assert_eq!(peers.active_peers_order()[0].0, 1);

		peers.on_blocks_requested(2, &vec![H256::default()]);
		assert_eq!(peers.active_peers_order().len(), 2);
		assert_eq!(peers.active_peers_order()[0].0, 1);
		assert_eq!(peers.active_peers_order()[1].0, 2);

		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 2);

		peers.reset_tasks(1);

		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		assert_eq!(peers.active_peers_order().len(), 1);
		assert_eq!(peers.active_peers_order()[0].0, 2);

		for _ in 0..MAX_PEER_FAILURES {
			peers.on_peer_failure(2);
		}

		assert_eq!(peers.active_peers_order().len(), 0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 1);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peer_not_inserted_when_known() {
		// TODO
	}
}
