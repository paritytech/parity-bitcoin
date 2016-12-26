use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use linked_hash_map::LinkedHashMap;
use time::precise_time_s;
use primitives::hash::H256;
use types::PeerIndex;

/// Max peer failures # before excluding from sync process
const MAX_PEER_FAILURES: usize = 2;
/// Max blocks failures # before forgetiing this block and restarting sync
const MAX_BLOCKS_FAILURES: usize = 6;

/// Set of peers selected for synchronization.
#[derive(Debug)]
pub struct PeersTasks {
	/// Peers that are marked as useful for current synchronization session && have no pending requests.
	idle: HashSet<PeerIndex>,
	/// Peers that are marked as non-useful for current synchronization session && have no pending requests.
	unuseful: HashSet<PeerIndex>,
	/// # of failures for given peer.
	failures: HashMap<PeerIndex, usize>,
	/// # of failures for given block.
	blocks_failures: HashMap<H256, usize>,
	/// Peers that are marked as useful for current synchronization session && have pending blocks requests.
	blocks_requests: HashMap<PeerIndex, HashSet<H256>>,
	/// Last block message time from peer.
	blocks_requests_order: LinkedHashMap<PeerIndex, f64>,
	/// Peers that are marked as useful for current synchronization session && have pending requests.
	inventory_requests: HashSet<PeerIndex>,
	/// Last inventory message time from peer.
	inventory_requests_order: LinkedHashMap<PeerIndex, f64>,
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

impl PeersTasks {
	pub fn new() -> Self {
		PeersTasks {
			idle: HashSet::new(),
			unuseful: HashSet::new(),
			failures: HashMap::new(),
			blocks_failures: HashMap::new(),
			blocks_requests: HashMap::new(),
			blocks_requests_order: LinkedHashMap::new(),
			inventory_requests: HashSet::new(),
			inventory_requests_order: LinkedHashMap::new(),
		}
	}

	/// Get information on synchronization peers
	#[cfg(test)]
	pub fn information(&self) -> Information {
		let blocks_requests_peers: HashSet<_> = self.blocks_requests.keys().cloned().collect();
		let total_unuseful_peers = self.unuseful.difference(&self.inventory_requests).count();
		let total_active_peers = blocks_requests_peers.union(&self.inventory_requests).count();
		Information {
			idle: self.idle.len(),
			unuseful: total_unuseful_peers,
			active: total_active_peers,
		}
	}

	/// Get all peers
	pub fn all_peers(&self) -> Vec<PeerIndex> {
		let mut unique: Vec<_> = self.idle.iter().cloned()
			.chain(self.unuseful.iter().cloned())
			.chain(self.blocks_requests.keys().cloned())
			.chain(self.inventory_requests.iter().cloned())
			.collect();
		// need stable (for tests) && unique peers here, as blocks_requests can intersect with inventory_requests
		unique.sort();
		unique.dedup();
		unique
	}

	/// Get useful peers
	pub fn useful_peers(&self) -> Vec<PeerIndex> {
		let mut unique: Vec<_> = self.idle.iter().cloned()
			.chain(self.blocks_requests.keys().cloned())
			.chain(self.inventory_requests.iter().cloned())
			.collect();
		// need stable (for tests) && unique peers here, as blocks_requests can intersect with inventory_requests
		unique.sort();
		unique.dedup();
		unique
	}

	/// Get idle peers for inventory request.
	pub fn idle_peers_for_inventory(&self) -> Vec<PeerIndex> {
		let peers: HashSet<_> = self.idle.iter().cloned()
			.chain(self.blocks_requests.keys().cloned())
			.collect();
		let except: HashSet<_> = self.inventory_requests.iter().cloned().collect();
		peers.difference(&except).cloned().collect()
	}

	/// Get idle peers for blocks request.
	pub fn idle_peers_for_blocks(&self) -> Vec<PeerIndex> {
		let peers: HashSet<_> = self.idle.iter().cloned()
			.chain(self.inventory_requests.iter().cloned())
			.collect();
		let except: HashSet<_> = self.blocks_requests.keys().cloned().collect();
		peers.difference(&except).cloned().collect()
	}

	/// Get active blocks requests, sorted by last response time (oldest first).
	pub fn ordered_blocks_requests(&self) -> Vec<(PeerIndex, f64)> {
		self.blocks_requests_order.iter()
			.map(|(&pi, &t)| (pi, t))
			.collect()
	}

	/// Get active inventory requests, sorted by last response time (oldest first).
	pub fn ordered_inventory_requests(&self) -> Vec<(PeerIndex, f64)> {
		self.inventory_requests_order.iter()
			.map(|(&pi, &t)| (pi, t))
			.collect()
	}

	/// Get peer tasks
	pub fn get_blocks_tasks(&self, peer_index: PeerIndex) -> Option<HashSet<H256>> {
		self.blocks_requests.get(&peer_index).cloned()
	}

	/// Mark peer as useful.
	pub fn useful_peer(&mut self, peer_index: PeerIndex) {
		// if peer is unknown => insert to idle queue
		// if peer is known && not useful => insert to idle queue
		if !self.idle.contains(&peer_index)
			&& !self.blocks_requests.contains_key(&peer_index)
			&& !self.inventory_requests.contains(&peer_index) {
			self.idle.insert(peer_index);
			self.unuseful.remove(&peer_index);
			self.failures.remove(&peer_index);
		}
	}

	/// Mark peer as unuseful.
	pub fn unuseful_peer(&mut self, peer_index: PeerIndex) {
		// if peer is unknown => insert to idle queue
		// if peer is known && not useful => insert to idle queue
		assert!(!self.blocks_requests.contains_key(&peer_index));
		assert!(!self.blocks_requests_order.contains_key(&peer_index));

		self.idle.remove(&peer_index);
		self.unuseful.insert(peer_index);
		self.failures.remove(&peer_index);
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);
	}

	/// Peer has been disconnected
	pub fn disconnect(&mut self, peer_index: PeerIndex) {
		// forget this peer without any chances to reuse
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.failures.remove(&peer_index);
		self.blocks_requests.remove(&peer_index);
		self.blocks_requests_order.remove(&peer_index);
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: PeerIndex, block_hash: &H256) {
		// forget block failures
		self.blocks_failures.remove(block_hash);

		// if this is requested block && it is last requested block => remove from blocks_requests
		let try_mark_as_idle = match self.blocks_requests.entry(peer_index) {
			Entry::Occupied(mut requests_entry) => { 
				requests_entry.get_mut().remove(block_hash);
				self.blocks_requests_order.remove(&peer_index);
				if requests_entry.get().is_empty() {
					requests_entry.remove_entry();
					true
				} else {
					self.blocks_requests_order.insert(peer_index, precise_time_s());
					false
				}
			},
			_ => false,
		};

		// try to mark as idle
		if try_mark_as_idle {
			self.try_mark_idle(peer_index);
		}
	}

	/// Inventory received from peer.
	pub fn on_inventory_received(&mut self, peer_index: PeerIndex) {
		// if we have requested inventory => remove from inventory_requests
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);

		// try to mark as idle
		self.try_mark_idle(peer_index);
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: PeerIndex, blocks_hashes: &[H256]) {
		// mark peer as active
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.blocks_requests.entry(peer_index).or_insert_with(HashSet::new).extend(blocks_hashes.iter().cloned());
		self.blocks_requests_order.remove(&peer_index);
		self.blocks_requests_order.insert(peer_index, precise_time_s());
	}

	/// Inventory has been requested from peer.
	pub fn on_inventory_requested(&mut self, peer_index: PeerIndex) {
		self.inventory_requests.insert(peer_index);
		self.inventory_requests_order.remove(&peer_index);
		self.inventory_requests_order.insert(peer_index, precise_time_s());

		// mark peer as active
		if self.idle.remove(&peer_index) {
			self.unuseful.insert(peer_index);
		}
		// peer is now out-of-synchronization process (will not request blocks from him), because:
		// 1) if it has new blocks, it will respond with `inventory` message && will be inserted back here
		// 2) if it has no new blocks => either synchronization is completed, or it is behind us in sync
	}

	/// We have failed to get blocks
	pub fn on_blocks_failure(&mut self, hashes: Vec<H256>) -> (Vec<H256>, Vec<H256>) {
		let mut failed_blocks: Vec<H256> = Vec::new();
		let mut normal_blocks: Vec<H256> = Vec::with_capacity(hashes.len());
		for hash in hashes {
			match self.blocks_failures.entry(hash.clone()) {
				Entry::Vacant(entry) => {
					normal_blocks.push(hash);
					entry.insert(0);
				},
				Entry::Occupied(mut entry) => {
					*entry.get_mut() += 1;
					if *entry.get() >= MAX_BLOCKS_FAILURES {
						entry.remove();
						failed_blocks.push(hash);
					} else {
						normal_blocks.push(hash);
					}
				}
			}
		}

		(normal_blocks, failed_blocks)
	}

	/// We have failed to get block from peer during given period
	pub fn on_peer_block_failure(&mut self, peer_index: PeerIndex) -> bool {
		let peer_failures = match self.failures.entry(peer_index) {
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
			self.unuseful.insert(peer_index);
			self.failures.remove(&peer_index);
			self.blocks_requests.remove(&peer_index);
			self.blocks_requests_order.remove(&peer_index);
		}
		too_much_failures
	}

	/// We have failed to get inventory from peer during given period
	pub fn on_peer_inventory_failure(&mut self, peer_index: PeerIndex) {
		// ignore inventory failures
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);

		if !self.blocks_requests.contains_key(&peer_index) {
			self.idle.insert(peer_index);
			self.unuseful.remove(&peer_index);
		}
	}

	/// Reset all peers state to the unuseful
	pub fn reset(&mut self) {
		self.unuseful.extend(self.idle.drain());
		self.unuseful.extend(self.blocks_requests.drain().map(|(k, _)| k));
		self.unuseful.extend(self.inventory_requests.drain());
		self.failures.clear();
		self.inventory_requests_order.clear();
		self.blocks_requests_order.clear();
	}

	/// Reset peer tasks && move peer to idle state
	pub fn reset_blocks_tasks(&mut self, peer_index: PeerIndex) -> Vec<H256> {
		let requests = self.blocks_requests.remove(&peer_index);
		self.blocks_requests_order.remove(&peer_index);
		self.try_mark_idle(peer_index);
		requests.unwrap_or_default().into_iter().collect()
	}

	/// Try to mark peer as idle
	fn try_mark_idle(&mut self, peer_index: PeerIndex) {
		if self.blocks_requests.contains_key(&peer_index)
			|| self.inventory_requests.contains(&peer_index) {
			return;
		}

		self.idle.insert(peer_index);
		self.unuseful.remove(&peer_index);
	}
}

#[cfg(test)]
mod tests {
	use super::{PeersTasks, MAX_PEER_FAILURES, MAX_BLOCKS_FAILURES};
	use primitives::hash::H256;

	#[test]
	fn peers_empty_on_start() {
		let peers = PeersTasks::new();
		assert_eq!(peers.idle_peers_for_blocks(), vec![]);
		assert_eq!(peers.idle_peers_for_inventory(), vec![]);

		let info = peers.information();
		assert_eq!(info.idle, 0);
		assert_eq!(info.active, 0);
	}

	#[test]
	fn peers_all_unuseful_after_reset() {
		let mut peers = PeersTasks::new();
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
		let mut peers = PeersTasks::new();
		peers.on_blocks_requested(7, &vec![H256::default()]);
		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);
		assert_eq!(peers.reset_blocks_tasks(7), vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_active_after_inventory_request() {
		let mut peers = PeersTasks::new();
		peers.useful_peer(5);
		peers.useful_peer(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		peers.on_inventory_requested(5);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);
	}

	#[test]
	fn peers_insert_remove_idle() {
		let mut peers = PeersTasks::new();

		peers.useful_peer(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers_for_blocks(), vec![0]);

		peers.useful_peer(5);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		let idle_peers = peers.idle_peers_for_blocks();
		assert!(idle_peers[0] == 0 || idle_peers[0] == 5);
		assert!(idle_peers[1] == 0 || idle_peers[1] == 5);

		peers.disconnect(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		let idle_peers = peers.idle_peers_for_blocks();
		assert!(idle_peers[0] == 0 || idle_peers[0] == 5);
		assert!(idle_peers[1] == 0 || idle_peers[1] == 5);

		peers.disconnect(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers_for_blocks(), vec![5]);
	}

	#[test]
	fn peers_request_blocks() {
		let mut peers = PeersTasks::new();

		peers.useful_peer(5);

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
		let mut peers = PeersTasks::new();

		peers.useful_peer(1);
		peers.useful_peer(2);
		assert_eq!(peers.ordered_blocks_requests(), vec![]);

		peers.on_blocks_requested(1, &vec![H256::default()]);
		assert_eq!(peers.ordered_blocks_requests().len(), 1);
		assert_eq!(peers.ordered_blocks_requests()[0].0, 1);

		peers.on_blocks_requested(2, &vec![H256::default()]);
		assert_eq!(peers.ordered_blocks_requests().len(), 2);
		assert_eq!(peers.ordered_blocks_requests()[0].0, 1);
		assert_eq!(peers.ordered_blocks_requests()[1].0, 2);

		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 2);

		peers.reset_blocks_tasks(1);

		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		assert_eq!(peers.ordered_blocks_requests().len(), 1);
		assert_eq!(peers.ordered_blocks_requests()[0].0, 2);

		for _ in 0..MAX_PEER_FAILURES {
			peers.on_peer_block_failure(2);
		}

		assert_eq!(peers.ordered_blocks_requests().len(), 0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 1);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peer_not_inserted_when_known() {
		let mut peers = PeersTasks::new();
		peers.useful_peer(1);
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
		peers.on_blocks_requested(1, &vec![H256::default()]);
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
		for _ in 0..MAX_PEER_FAILURES {
			peers.on_peer_block_failure(1);
		}
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
	}

	#[test]
	fn peer_block_failures() {
		let mut peers = PeersTasks::new();
		peers.useful_peer(1);
		peers.on_blocks_requested(1, &vec![H256::from(1)]);
		for _ in 0..MAX_BLOCKS_FAILURES {
			let requested_blocks = peers.reset_blocks_tasks(1);
			let (blocks_to_request, blocks_to_forget) = peers.on_blocks_failure(requested_blocks);
			assert_eq!(blocks_to_request, vec![H256::from(1)]);
			assert_eq!(blocks_to_forget, vec![]);
			peers.on_blocks_requested(1, &vec![H256::from(1)]);
		}
		let requested_blocks = peers.reset_blocks_tasks(1);
		let (blocks_to_request, blocks_to_forget) = peers.on_blocks_failure(requested_blocks);
		assert_eq!(blocks_to_request, vec![]);
		assert_eq!(blocks_to_forget, vec![H256::from(1)]);
	}
}
