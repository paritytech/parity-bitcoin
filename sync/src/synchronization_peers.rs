use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use primitives::hash::H256;
use linked_hash_map::LinkedHashMap;
use time::precise_time_s;
use connection_filter::ConnectionFilter;
use synchronization_client::BlockAnnouncementType;

/// Max peer failures # before excluding from sync process
const MAX_PEER_FAILURES: usize = 2;

/// Set of peers selected for synchronization.
#[derive(Debug)]
pub struct Peers {
	/// Peers that are marked as useful for current synchronization session && have no pending requests.
	idle: HashSet<usize>,
	/// Peers that are marked as non-useful for current synchronization session && have no pending requests.
	unuseful: HashSet<usize>,
	/// # of failures for given peer.
	failures: HashMap<usize, usize>,
	/// Peers that are marked as useful for current synchronization session && have pending blocks requests.
	blocks_requests: HashMap<usize, HashSet<H256>>,
	/// Last block message time from peer.
	blocks_requests_order: LinkedHashMap<usize, f64>,
	/// Peers that are marked as useful for current synchronization session && have pending requests.
	inventory_requests: HashSet<usize>,
	/// Last inventory message time from peer.
	inventory_requests_order: LinkedHashMap<usize, f64>,
	/// Peer connections filters.
	filters: HashMap<usize, ConnectionFilter>,
	/// The way peer is informed about new blocks
	block_announcement_types: HashMap<usize, BlockAnnouncementType>,
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
			failures: HashMap::new(),
			blocks_requests: HashMap::new(),
			blocks_requests_order: LinkedHashMap::new(),
			inventory_requests: HashSet::new(),
			inventory_requests_order: LinkedHashMap::new(),
			filters: HashMap::new(),
			block_announcement_types: HashMap::new(),
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

	/// Is known peer
	pub fn is_known_peer(&self, peer_index: usize) -> bool {
		self.idle.contains(&peer_index)
			|| self.unuseful.contains(&peer_index)
			|| self.blocks_requests.contains_key(&peer_index)
			|| self.inventory_requests.contains(&peer_index)
	}

	/// Has any useful peers?
	pub fn has_any_useful(&self) -> bool {
		!self.idle.is_empty()
			|| !self.blocks_requests.is_empty()
			|| !self.inventory_requests.is_empty()
	}

	/// Get all peers
	pub fn all_peers(&self) -> Vec<usize> {
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
	pub fn useful_peers(&self) -> Vec<usize> {
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
	pub fn idle_peers_for_inventory(&self) -> Vec<usize> {
		let peers: HashSet<_> = self.idle.iter().cloned()
			.chain(self.blocks_requests.keys().cloned())
			.collect();
		let except: HashSet<_> = self.inventory_requests.iter().cloned().collect();
		peers.difference(&except).cloned().collect()
	}

	/// Get idle peers.
	pub fn idle_peers_for_blocks(&self) -> Vec<usize> {
		let peers: HashSet<_> = self.idle.iter().cloned()
			.chain(self.inventory_requests.iter().cloned())
			.collect();
		let except: HashSet<_> = self.blocks_requests.keys().cloned().collect();
		peers.difference(&except).cloned().collect()
	}

	/// Get active blocks requests, sorted by last response time (oldest first).
	pub fn ordered_blocks_requests(&self) -> Vec<(usize, f64)> {
		self.blocks_requests_order.iter()
			.map(|(&pi, &t)| (pi, t))
			.collect()
	}

	/// Get active inventory requests, sorted by last response time (oldest first).
	pub fn ordered_inventory_requests(&self) -> Vec<(usize, f64)> {
		self.inventory_requests_order.iter()
			.map(|(&pi, &t)| (pi, t))
			.collect()
	}

	/// Get peer tasks
	pub fn get_blocks_tasks(&self, peer_index: usize) -> Option<HashSet<H256>> {
		self.blocks_requests.get(&peer_index).cloned()
	}

	/// Get filter reference for given peer
	pub fn filter(&mut self, peer_index: usize) -> &ConnectionFilter {
		assert!(self.is_known_peer(peer_index));
		&*self.filters.entry(peer_index).or_insert_with(ConnectionFilter::default)
	}

	/// Get mutable filter reference for given peer
	pub fn filter_mut(&mut self, peer_index: usize) -> &mut ConnectionFilter {
		assert!(self.is_known_peer(peer_index));
		self.filters.entry(peer_index).or_insert_with(ConnectionFilter::default)
	}

	/// Get the way peer is informed about new blocks
	pub fn block_announcement_type(&self, peer_index: usize) -> BlockAnnouncementType {
		self.block_announcement_types.get(&peer_index).cloned()
			.unwrap_or(BlockAnnouncementType::SendInventory)
	}

	/// Mark peer as useful.
	pub fn useful_peer(&mut self, peer_index: usize) {
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
	pub fn unuseful_peer(&mut self, peer_index: usize) {
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

	/// Change the way peer is informed about new blocks
	pub fn set_block_announcement_type(&mut self, peer_index: usize, announcement_type: BlockAnnouncementType) {
		self.block_announcement_types.insert(peer_index, announcement_type);
	}

	/// Peer has been disconnected
	pub fn on_peer_disconnected(&mut self, peer_index: usize) -> Option<Vec<H256>> {
		// forget this peer without any chances to reuse
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.failures.remove(&peer_index);
		let peer_blocks_requests = self.blocks_requests.remove(&peer_index);
		self.blocks_requests_order.remove(&peer_index);
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);
		self.filters.remove(&peer_index);
		self.block_announcement_types.remove(&peer_index);
		peer_blocks_requests
			.map(|hs| hs.into_iter().collect())
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: usize, block_hash: &H256) {
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

		// remember that peer knows about this block
		self.filters.entry(peer_index).or_insert_with(ConnectionFilter::default).known_block(block_hash);
	}

	/// Transaction is received from peer.
	pub fn on_transaction_received(&mut self, peer_index: usize, transaction_hash: &H256) {
		self.filters.entry(peer_index).or_insert_with(ConnectionFilter::default).known_transaction(transaction_hash);
	}

	/// Inventory received from peer.
	pub fn on_inventory_received(&mut self, peer_index: usize) {
		// if we have requested inventory => remove from inventory_requests
		self.inventory_requests.remove(&peer_index);
		self.inventory_requests_order.remove(&peer_index);

		// try to mark as idle
		self.try_mark_idle(peer_index);
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: usize, blocks_hashes: &[H256]) {
		// mark peer as active
		self.idle.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.blocks_requests.entry(peer_index).or_insert_with(HashSet::new).extend(blocks_hashes.iter().cloned());
		self.blocks_requests_order.remove(&peer_index);
		self.blocks_requests_order.insert(peer_index, precise_time_s());
	}

	/// Inventory has been requested from peer.
	pub fn on_inventory_requested(&mut self, peer_index: usize) {
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

	/// We have failed to get block from peer during given period
	pub fn on_peer_block_failure(&mut self, peer_index: usize) -> bool {
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
	pub fn on_peer_inventory_failure(&mut self, peer_index: usize) {
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
	pub fn reset_blocks_tasks(&mut self, peer_index: usize) -> Vec<H256> {
		let requests = self.blocks_requests.remove(&peer_index);
		self.blocks_requests_order.remove(&peer_index);
		self.try_mark_idle(peer_index);
		requests.expect("empty requests queue is not allowed").into_iter().collect()
	}

	/// Try to mark peer as idle
	fn try_mark_idle(&mut self, peer_index: usize) {
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
	use super::{Peers, MAX_PEER_FAILURES};
	use primitives::hash::H256;

	#[test]
	fn peers_empty_on_start() {
		let peers = Peers::new();
		assert_eq!(peers.idle_peers_for_blocks(), vec![]);
		assert_eq!(peers.idle_peers_for_inventory(), vec![]);

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
		assert_eq!(peers.reset_blocks_tasks(7), vec![H256::default()]);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peers_active_after_inventory_request() {
		let mut peers = Peers::new();
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
		let mut peers = Peers::new();

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

		peers.on_peer_disconnected(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		let idle_peers = peers.idle_peers_for_blocks();
		assert!(idle_peers[0] == 0 || idle_peers[0] == 5);
		assert!(idle_peers[1] == 0 || idle_peers[1] == 5);

		peers.on_peer_disconnected(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers_for_blocks(), vec![5]);
	}

	#[test]
	fn peers_request_blocks() {
		let mut peers = Peers::new();

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
		let mut peers = Peers::new();

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
		let mut peers = Peers::new();
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
}
