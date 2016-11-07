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
	/// Peers that have no pending requests.
	idle: HashSet<usize>,
	/// Pending requests by peer.
	requests: HashMap<usize, HashSet<H256>>,
	/// Peers failures.
	failures: HashMap<usize, usize>,
	/// Last message time from peer
	times: LinkedHashMap<usize, f64>,
}

/// Information on synchronization peers
#[cfg(test)]
#[derive(Debug)]
pub struct Information {
	/// Number of currently idle synchronization peers.
	pub idle: usize,
	/// Number of currently active synchronization peers.
	pub active: usize,
}

impl Peers {
	pub fn new() -> Peers {
		Peers {
			idle: HashSet::new(),
			requests: HashMap::new(),
			failures: HashMap::new(),
			times: LinkedHashMap::new(),
		}
	}

	/// Get information on synchronization peers
	#[cfg(test)]
	pub fn information(&self) -> Information {
		Information {
			idle: self.idle.len(),
			active: self.requests.len(),
		}
	}

	/// Get idle peer.
	#[cfg(test)]
	pub fn idle_peer(&self) -> Option<usize> {
		self.idle.iter().cloned().next()
	}

	/// Get idle peers.
	pub fn idle_peers(&self) -> Vec<usize> {
		self.idle.iter().cloned().collect()
	}

	/// Get worst peer.
	pub fn worst_peers(&self) -> Vec<(usize, f64)> {
		self.times.iter().map(|(&pi, &t)| (pi, t)).collect()
	}

	/// Insert new synchronization peer.
	pub fn insert(&mut self, peer_index: usize) {
		if !self.idle.contains(&peer_index) && !self.requests.contains_key(&peer_index) {
			self.idle.insert(peer_index);
		}
	}

	/// Peer has been disconnected
	pub fn on_peer_disconnected(&mut self, peer_index: usize) -> bool {
		self.idle.remove(&peer_index);
		self.requests.remove(&peer_index);
		self.failures.remove(&peer_index);
		self.times.remove(&peer_index);
		(self.idle.len() + self.requests.len()) == 0
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: usize, block_hash: &H256) {
		if let Entry::Occupied(mut entry) = self.requests.entry(peer_index) {
			entry.get_mut().remove(block_hash);
			if entry.get().is_empty() {
				self.idle.insert(peer_index);
				entry.remove_entry();
			}
		}
		self.on_peer_message(peer_index);
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: usize, blocks_hashes: &Vec<H256>) {
		// inventory can only be requested from idle peers
		assert!(!self.requests.contains_key(&peer_index));

		self.idle.remove(&peer_index);
		self.requests.entry(peer_index).or_insert(HashSet::new()).extend(blocks_hashes.iter().cloned());
		self.times.insert(peer_index, precise_time_s());
	}

	/// Inventory has been requested from peer.
	pub fn on_inventory_requested(&mut self, peer_index: usize) {
		// inventory can only be requested from idle peers
		assert!(!self.requests.contains_key(&peer_index));

		self.idle.remove(&peer_index);
		// peer is now out-of-synchronization process, because:
		// 1) if it has new blocks, it will respond with `inventory` message && will be inserted back here
		// 2) if it has no new blocks => either synchronization is completed, or it is behind us in sync
	}

	/// We have failed to get response from peer during given period
	pub fn on_peer_failure(&mut self, peer_index: usize) -> bool {
		let peer_failures = match self.failures.entry(peer_index) {
			Entry::Occupied(mut entry) => {
				let failures = entry.get() + 1;
				entry.insert(failures) + 1;
				failures
			},
			Entry::Vacant(entry) => *entry.insert(1),
		};

		let too_much_failures = peer_failures >= MAX_PEER_FAILURES;
		if too_much_failures {
			self.failures.remove(&peer_index);
			self.requests.remove(&peer_index);
			self.times.remove(&peer_index);
		}
		too_much_failures
	}

	/// Reset peers state
	pub fn reset(&mut self) {
		self.idle.extend(self.requests.drain().map(|(k, _)| k));
		self.failures.clear();
		self.times.clear();
	}

	/// Reset peer tasks
	pub fn reset_tasks(&mut self, peer_index: usize) {
		self.requests.remove(&peer_index);
		self.times.remove(&peer_index);
		self.idle.insert(peer_index);
	}

	/// When sync message is received from peer
	fn on_peer_message(&mut self, peer_index: usize) {
		self.failures.remove(&peer_index);
		self.times.remove(&peer_index);
		if self.requests.contains_key(&peer_index) {
			self.times.insert(peer_index, precise_time_s());
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
		assert_eq!(peers.idle_peer(), None);
		assert_eq!(peers.idle_peers().len(), 0);

		let info = peers.information();
		assert_eq!(info.idle, 0);
		assert_eq!(info.active, 0);
	}

	#[test]
	fn peers_all_idle_after_reset() {
		let mut peers = Peers::new();
		peers.on_blocks_requested(7, &vec![H256::default()]);
		peers.on_blocks_requested(8, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().active, 2);
		peers.reset();
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_removed_after_inventory_request() {
		let mut peers = Peers::new();
		peers.insert(5);
		peers.insert(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		peers.on_inventory_requested(5);
		assert_eq!(peers.information().idle, 1);
	}

	#[test]
	fn peers_insert_remove_idle() {
		let mut peers = Peers::new();

		peers.insert(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peer(), Some(0));
		assert_eq!(peers.idle_peers(), vec![0]);

		peers.insert(5);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		assert!(peers.idle_peer() == Some(0) || peers.idle_peer() == Some(5));
		assert!(peers.idle_peers()[0] == 0 || peers.idle_peers()[0] == 5);
		assert!(peers.idle_peers()[1] == 0 || peers.idle_peers()[1] == 5);

		peers.on_peer_disconnected(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		assert!(peers.idle_peer() == Some(0) || peers.idle_peer() == Some(5));
		assert!(peers.idle_peers()[0] == 0 || peers.idle_peers()[0] == 5);
		assert!(peers.idle_peers()[1] == 0 || peers.idle_peers()[1] == 5);

		peers.on_peer_disconnected(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peer(), Some(5));
		assert_eq!(peers.idle_peers(), vec![5]);
	}

	#[test]
	fn peers_request_blocks() {
		let mut peers = Peers::new();

		peers.insert(5);

		peers.on_blocks_requested(7, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 1);

		peers.on_blocks_requested(8, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 2);

		peers.on_block_received(7, &H256::default());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(9, &H256::default());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(8, &"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into());
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 1);

		peers.on_block_received(8, &H256::default());
		assert_eq!(peers.information().idle, 3);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_worst() {
		let mut peers = Peers::new();

		peers.insert(1);
		peers.insert(2);
		assert_eq!(peers.worst_peers(), vec![]);

		peers.on_blocks_requested(1, &vec![H256::default()]);
		assert_eq!(peers.worst_peers().len(), 1);
		assert_eq!(peers.worst_peers()[0].0, 1);

		peers.on_blocks_requested(2, &vec![H256::default()]);
		assert_eq!(peers.worst_peers().len(), 2);
		assert_eq!(peers.worst_peers()[0].0, 1);
		assert_eq!(peers.worst_peers()[1].0, 2);

		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().active, 2);

		peers.reset_tasks(1);

		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 1);

		assert_eq!(peers.worst_peers().len(), 1);
		assert_eq!(peers.worst_peers()[0].0, 2);

		for _ in 0..MAX_PEER_FAILURES {
			peers.on_peer_failure(2);
		}

		assert_eq!(peers.worst_peers().len(), 0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
	}
}
