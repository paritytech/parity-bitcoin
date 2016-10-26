use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use primitives::hash::H256;

// TODO: sync score for peers + choose peers based on their score

/// Set of peers selected for synchronization.
#[derive(Debug)]
pub struct Peers {
	/// Peers that have not pending blocks requests.
	idle_peers: HashSet<usize>,
	/// Pending block requests by peer.
	blocks_requests: HashMap<usize, HashSet<H256>>,
}

/// Information on synchronization peers
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
			idle_peers: HashSet::new(),
			blocks_requests: HashMap::new(),
		}
	}

	/// Get information on synchronization peers
	pub fn information(&self) -> Information {
		Information {
			idle: self.idle_peers.len(),
			active: self.blocks_requests.len(),
		}
	}

	/// Get idle peer.
	pub fn idle_peer(&self) -> Option<usize> {
		self.idle_peers.iter().cloned().next()
	}

	/// Get idle peers.
	pub fn idle_peers(&self) -> Vec<usize> {
		self.idle_peers.iter().cloned().collect()
	}

	/// Insert new synchronization peer.
	pub fn insert(&mut self, peer_index: usize) {
		if !self.idle_peers.contains(&peer_index) && !self.blocks_requests.contains_key(&peer_index) {
			self.idle_peers.insert(peer_index);
		}
	}

	/// Remove synchronization peer.
	#[cfg(test)]
	pub fn remove(&mut self, peer_index: usize) {
		self.idle_peers.remove(&peer_index);
		self.blocks_requests.remove(&peer_index);
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: usize, block_hash: &H256) {
		if let Entry::Occupied(mut entry) = self.blocks_requests.entry(peer_index) {
			entry.get_mut().remove(block_hash);
			if entry.get().is_empty() {
				self.idle_peers.insert(peer_index);
				entry.remove_entry();
			}
		}
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: usize, blocks_hashes: &Vec<H256>) {
		self.blocks_requests.entry(peer_index).or_insert(HashSet::new()).extend(blocks_hashes.iter().cloned());
		self.idle_peers.remove(&peer_index);
	}

	/// Inventory has been requested from peer.
	pub fn on_inventory_requested(&mut self, _peer_index: usize) {
		// TODO
	}
}

#[cfg(test)]
mod tests {
	use super::Peers;
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

		peers.remove(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		assert!(peers.idle_peer() == Some(0) || peers.idle_peer() == Some(5));
		assert!(peers.idle_peers()[0] == 0 || peers.idle_peers()[0] == 5);
		assert!(peers.idle_peers()[1] == 0 || peers.idle_peers()[1] == 5);

		peers.remove(0);
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
}