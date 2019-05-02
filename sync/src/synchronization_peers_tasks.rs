use std::fmt;
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use linked_hash_map::LinkedHashMap;
use time::precise_time_s;
use primitives::hash::H256;
use types::PeerIndex;
use utils::AverageSpeedMeter;

/// Max peer failures # before excluding from sync process
const MAX_PEER_FAILURES: usize = 4;
/// Max blocks failures # before forgetiing this block and restarting sync
const MAX_BLOCKS_FAILURES: usize = 6;
/// Number of blocks to inspect while calculating average response time
const BLOCKS_TO_INSPECT: usize = 32;

/// Information on synchronization peers
pub struct Information {
	/// total # of peers.
	pub all: usize,
	/// # of peers that are marked as useful for current synchronization session && have no pending requests.
	pub idle: usize,
	/// # of peers that are marked as non-useful for current synchronization session && have no pending requests.
	pub unuseful: usize,
	/// # of peers that are marked as useful for current synchronization session && have pending requests.
	pub active: usize,
}

/// Set of peers selected for synchronization.
#[derive(Debug, Default)]
pub struct PeersTasks {
	/// All known peers ids
	all: HashSet<PeerIndex>,
	/// All unuseful peers
	unuseful: HashSet<PeerIndex>,
	/// All peers without pending headers requests
	idle_for_headers: HashSet<PeerIndex>,
	/// All peers without pending blocks requests
	idle_for_blocks: HashSet<PeerIndex>,
	/// Pending headers requests sent to peers
	headers_requests: LinkedHashMap<PeerIndex, HeadersRequest>,
	/// Pending blocks requests sent to peers
	blocks_requests: LinkedHashMap<PeerIndex, BlocksRequest>,
	/// Peers statistics
	stats: HashMap<PeerIndex, PeerStats>,
	/// Blocks statistics
	blocks_stats: HashMap<H256, BlockStats>,
}

/// Pending headers request
#[derive(Debug, Clone)]
pub struct HeadersRequest {
	/// Time when request has been sent
	pub timestamp: f64,
}

/// Pending blocks request
#[derive(Debug, Clone)]
pub struct BlocksRequest {
	/// Time when request has been sent
	pub timestamp: f64,
	/// Hashes of blocks that have been requested
	pub blocks: HashSet<H256>,
}

/// Peer trust level.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum TrustLevel {
	/// Suspicios peer (either it is fresh peer, or it has failed to respond to last requests).
	Suspicious,
	/// This peer is responding to requests.
	Trusted,
}

/// Peer statistics
#[derive(Debug)]
pub struct PeerStats {
	/// Number of blocks requests failures
	failures: usize,
	/// Average block response time meter
	speed: AverageSpeedMeter,
	/// Peer trust level.
	trust: TrustLevel,
}

/// Block statistics
#[derive(Debug, Default)]
struct BlockStats {
	/// Number of block request failures
	failures: usize,
}

impl PeersTasks {
	/// Get information on synchronization peers
	pub fn information(&self) -> Information {
		let active_for_headers: HashSet<_> = self.headers_requests.keys().cloned().collect();
		Information {
			all: self.all.len(),
			idle: self.idle_for_blocks.difference(&active_for_headers).count(),
			unuseful: self.unuseful.len(),
			active: active_for_headers.union(&self.blocks_requests.keys().cloned().collect()).count(),
		}
	}

	/// Get all peers
	pub fn all_peers(&self) -> &HashSet<PeerIndex> {
		&self.all
	}

	/// Get useful peers
	pub fn useful_peers(&self) -> Vec<PeerIndex> {
		self.all.difference(&self.unuseful).cloned().collect()
	}

	/// Get idle peers for headers request.
	pub fn idle_peers_for_headers(&self) -> &HashSet<PeerIndex> {
		&self.idle_for_headers
	}

	/// Get idle peers for blocks request.
	pub fn idle_peers_for_blocks(&self) -> &HashSet<PeerIndex> {
		&self.idle_for_blocks
	}

	/// Sort peers for blocks request
	pub fn sort_peers_for_blocks(&self, peers: &mut Vec<PeerIndex>) {
		peers.sort_by(|left, right| {
			let left_speed = self.stats.get(left).map(|s| s.speed.speed()).unwrap_or(0f64);
			let right_speed = self.stats.get(right).map(|s| s.speed.speed()).unwrap_or(0f64);
			// larger speed => better
			right_speed.partial_cmp(&left_speed).unwrap_or(Ordering::Equal)
		})
	}

	/// Get active headers requests, sorted by last response time (oldest first).
	pub fn ordered_headers_requests(&self) -> &LinkedHashMap<PeerIndex, HeadersRequest> {
		&self.headers_requests
	}

	/// Get active blocks requests, sorted by last response time (oldest first).
	pub fn ordered_blocks_requests(&self) -> &LinkedHashMap<PeerIndex, BlocksRequest> {
		&self.blocks_requests
	}

	/// Get peer tasks
	pub fn get_blocks_tasks(&self, peer_index: PeerIndex) -> Option<&HashSet<H256>> {
		self.blocks_requests
			.get(&peer_index)
			.map(|br| &br.blocks)
	}

	/// Get peer statistics
	pub fn get_peer_stats(&self, peer_index: PeerIndex) -> Option<&PeerStats> {
		self.stats.get(&peer_index)
	}

	/// Get mutable reference to peer statistics
	#[cfg(test)]
	pub fn get_peer_stats_mut(&mut self, peer_index: PeerIndex) -> Option<&mut PeerStats> {
		self.stats.get_mut(&peer_index)
	}

	/// Mark peer as useful.
	pub fn useful_peer(&mut self, peer_index: PeerIndex) {
		// if peer is unknown => insert to idle queue
		// if peer is known && not useful => insert to idle queue
		if self.all.insert(peer_index)
			|| self.unuseful.remove(&peer_index) {
			self.idle_for_headers.insert(peer_index);
			self.idle_for_blocks.insert(peer_index);
			self.stats.insert(peer_index, PeerStats::new());
		}
	}

	/// Mark peer as unuseful.
	pub fn unuseful_peer(&mut self, peer_index: PeerIndex) {
		// blocks should be rerequested from another peers
		assert!(!self.blocks_requests.contains_key(&peer_index));

		if self.all.insert(peer_index) {
			self.stats.insert(peer_index, PeerStats::new());
		}
		self.unuseful.insert(peer_index);
		self.idle_for_headers.remove(&peer_index);
		self.idle_for_blocks.remove(&peer_index);
	}

	/// Peer has been disconnected
	pub fn disconnect(&mut self, peer_index: PeerIndex) {
		// blocks should be rerequested from another peers
		assert!(!self.blocks_requests.contains_key(&peer_index));

		self.all.remove(&peer_index);
		self.unuseful.remove(&peer_index);
		self.idle_for_headers.remove(&peer_index);
		self.idle_for_blocks.remove(&peer_index);
		self.headers_requests.remove(&peer_index);
		self.blocks_requests.remove(&peer_index);
		self.stats.remove(&peer_index);
	}

	/// Block is received from peer.
	pub fn on_block_received(&mut self, peer_index: PeerIndex, block_hash: &H256) {
		// block received => reset failures
		self.blocks_stats.remove(block_hash);

		let is_last_requested_block_received = if let Some(blocks_request) = self.blocks_requests.get_mut(&peer_index) {
			// if block hasn't been requested => do nothing
			if !blocks_request.blocks.remove(block_hash) {
				return;
			}

			blocks_request.blocks.is_empty()
		} else {
			// this peers hasn't been requested for blocks at all
			return;
		};

		// it was requested block => update block response time
		self.stats.get_mut(&peer_index)
			.map(|br| {
				if br.failures > 0 {
					br.failures -= 1;
				}
				br.trust = TrustLevel::Trusted;
				br.speed.checkpoint()
			});

		// if it hasn't been last requested block => just return
		if !is_last_requested_block_received {
			let mut peer_blocks_requests = self.blocks_requests.remove(&peer_index).expect("checked above; qed");
			peer_blocks_requests.timestamp = precise_time_s();
			self.blocks_requests.insert(peer_index, peer_blocks_requests);
			return;
		}

		// no more requested blocks => pause requests speed meter
		self.stats.get_mut(&peer_index).map(|br| br.speed.stop());

		// mark this peer as idle for blocks request
		self.blocks_requests.remove(&peer_index);
		self.idle_for_blocks.insert(peer_index);
		// also mark as available for headers request if not yet
		if !self.headers_requests.contains_key(&peer_index) {
			self.idle_for_headers.insert(peer_index);
		}
	}

	/// Headers received from peer.
	pub fn on_headers_received(&mut self, peer_index: PeerIndex) {
		self.headers_requests.remove(&peer_index);
		// we only ask for new headers when peer is also not asked for blocks
		// => only insert to idle queue if no active blocks requests
		if !self.blocks_requests.contains_key(&peer_index) {
			self.idle_for_headers.insert(peer_index);
		}
	}

	/// Blocks have been requested from peer.
	pub fn on_blocks_requested(&mut self, peer_index: PeerIndex, blocks_hashes: &[H256]) {
		if !self.all.contains(&peer_index) {
			self.unuseful_peer(peer_index);
		}

		self.unuseful.remove(&peer_index);
		self.idle_for_blocks.remove(&peer_index);

		if !self.blocks_requests.contains_key(&peer_index) {
			self.blocks_requests.insert(peer_index, BlocksRequest::new());
		}
		self.blocks_requests.get_mut(&peer_index)
			.expect("inserted one line above")
			.blocks.extend(blocks_hashes.iter().cloned());

		// no more requested blocks => pause requests speed meter
		self.stats.get_mut(&peer_index).map(|br| br.speed.start());
	}

	/// Headers hashave been requested from peer.
	pub fn on_headers_requested(&mut self, peer_index: PeerIndex) {
		if !self.all.contains(&peer_index) {
			self.unuseful_peer(peer_index);
		}

		self.idle_for_headers.remove(&peer_index);
		self.headers_requests.remove(&peer_index);
		self.headers_requests.insert(peer_index, HeadersRequest::new());
	}

	/// We have failed to get blocks
	pub fn on_blocks_failure(&mut self, hashes: Vec<H256>) -> (Vec<H256>, Vec<H256>) {
		let mut failed_blocks: Vec<H256> = Vec::new();
		let mut normal_blocks: Vec<H256> = Vec::with_capacity(hashes.len());
		for hash in hashes {
			let is_failed_block = {
				let block_stats = self.blocks_stats.entry(hash.clone()).or_insert_with(BlockStats::default);
				block_stats.failures += 1;
				block_stats.failures > MAX_BLOCKS_FAILURES
			};
			if is_failed_block {
				self.blocks_stats.remove(&hash);
				failed_blocks.push(hash);
			} else {
				normal_blocks.push(hash);
			}
		}

		(normal_blocks, failed_blocks)
	}

	/// We have failed to get block from peer during given period
	pub fn on_peer_block_failure(&mut self, peer_index: PeerIndex) -> bool {
		self.penalize(peer_index)
	}

	/// We have failed to get headers from peer during given period
	pub fn on_peer_headers_failure(&mut self, peer_index: PeerIndex) -> bool {
		// we never penalize peers for header requests failures
		self.headers_requests.remove(&peer_index);
		self.idle_for_headers.insert(peer_index);

		self.penalize(peer_index)
	}

	/// Penalize peer. Returns true if the peer score is too low to keep connection.
	pub fn penalize(&mut self, peer_index: PeerIndex) -> bool {
		self.stats.get_mut(&peer_index)
			.map(|s| {
				if s.trust == TrustLevel::Trusted {
					s.failures += 1;
					s.failures > MAX_PEER_FAILURES
				} else {
					s.failures = MAX_PEER_FAILURES;
					true
				}
			})
			.unwrap_or_default()
	}

	/// Reset all peers state to the unuseful
	pub fn reset(&mut self) {
		self.unuseful.clear();
		self.unuseful.extend(self.all.iter().cloned());
		self.idle_for_headers.clear();
		self.idle_for_blocks.clear();
		self.headers_requests.clear();
		self.blocks_requests.clear();
	}

	/// Reset peer tasks && move peer to idle state
	pub fn reset_blocks_tasks(&mut self, peer_index: PeerIndex) -> Vec<H256> {
		self.idle_for_blocks.insert(peer_index);
		self.blocks_requests.remove(&peer_index)
			.map(|mut br| br.blocks.drain().collect())
			.unwrap_or_default()
	}
}

impl HeadersRequest {
	pub fn new() -> Self {
		HeadersRequest {
			timestamp: precise_time_s(),
		}
	}
}

impl BlocksRequest {
	pub fn new() -> Self {
		BlocksRequest {
			timestamp: precise_time_s(),
			blocks: HashSet::new(),
		}
	}
}

impl PeerStats {
	pub fn new() -> Self {
		PeerStats {
			failures: 0,
			speed: AverageSpeedMeter::with_inspect_items(BLOCKS_TO_INSPECT),
			trust: TrustLevel::Suspicious,
		}
	}

	pub fn trust(&self) -> TrustLevel {
		self.trust
	}

	#[cfg(test)]
	pub fn set_trust(&mut self, trust: TrustLevel) {
		self.trust = trust;
	}
}

impl fmt::Debug for Information {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{} (act: {}, idl: {}, bad: {})", self.all, self.active, self.idle, self.unuseful)
	}
}

#[cfg(test)]
mod tests {
	use primitives::hash::H256;
	use super::{PeersTasks, MAX_PEER_FAILURES, MAX_BLOCKS_FAILURES};
	use types::PeerIndex;

	#[test]
	fn peers_empty_on_start() {
		let peers = PeersTasks::default();
		assert_eq!(peers.idle_peers_for_blocks().len(), 0);
		assert_eq!(peers.idle_peers_for_headers().len(), 0);

		let info = peers.information();
		assert_eq!(info.idle, 0);
		assert_eq!(info.active, 0);
	}

	#[test]
	fn peers_all_unuseful_after_reset() {
		let mut peers = PeersTasks::default();
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
		let mut peers = PeersTasks::default();
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
	fn peers_active_after_headers_request() {
		let mut peers = PeersTasks::default();
		peers.useful_peer(5);
		peers.useful_peer(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		peers.on_headers_requested(5);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);
	}

	#[test]
	fn peers_insert_remove_idle() {
		let mut peers = PeersTasks::default();

		peers.useful_peer(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers_for_blocks().len(), 1);
		assert!(peers.idle_peers_for_blocks().contains(&0));

		peers.useful_peer(5);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		let idle_peers: Vec<_> = peers.idle_peers_for_blocks().iter().cloned().collect();
		assert!(idle_peers[0] == 0 || idle_peers[0] == 5);
		assert!(idle_peers[1] == 0 || idle_peers[1] == 5);

		peers.disconnect(7);
		assert_eq!(peers.information().idle, 2);
		assert_eq!(peers.information().active, 0);
		let idle_peers: Vec<_> = peers.idle_peers_for_blocks().iter().cloned().collect();
		assert!(idle_peers[0] == 0 || idle_peers[0] == 5);
		assert!(idle_peers[1] == 0 || idle_peers[1] == 5);

		peers.disconnect(0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().active, 0);
		assert_eq!(peers.idle_peers_for_blocks().len(), 1);
		assert!(peers.idle_peers_for_blocks().contains(&5));
	}

	#[test]
	fn peers_request_blocks() {
		let mut peers = PeersTasks::default();

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
		let mut peers = PeersTasks::default();

		peers.useful_peer(1);
		peers.useful_peer(2);
		assert_eq!(peers.ordered_blocks_requests().len(), 0);

		peers.on_blocks_requested(1, &vec![H256::default()]);
		assert_eq!(peers.ordered_blocks_requests().len(), 1);
		assert_eq!(*peers.ordered_blocks_requests().keys().nth(0).unwrap(), 1);

		peers.on_blocks_requested(2, &vec![H256::default()]);
		assert_eq!(peers.ordered_blocks_requests().len(), 2);
		assert_eq!(*peers.ordered_blocks_requests().keys().nth(0).unwrap(), 1);
		assert_eq!(*peers.ordered_blocks_requests().keys().nth(1).unwrap(), 2);

		assert_eq!(peers.information().idle, 0);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 2);

		peers.reset_blocks_tasks(1);

		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 0);
		assert_eq!(peers.information().active, 1);

		assert_eq!(peers.ordered_blocks_requests().len(), 1);
		assert_eq!(*peers.ordered_blocks_requests().keys().nth(0).unwrap(), 2);

		for _ in 0..(MAX_PEER_FAILURES + 1) {
			if peers.on_peer_block_failure(2) {
				peers.reset_blocks_tasks(2);
				peers.unuseful_peer(2);
			}
		}

		assert_eq!(peers.ordered_blocks_requests().len(), 0);
		assert_eq!(peers.information().idle, 1);
		assert_eq!(peers.information().unuseful, 1);
		assert_eq!(peers.information().active, 0);
	}

	#[test]
	fn peer_not_inserted_when_known() {
		let mut peers = PeersTasks::default();
		peers.useful_peer(1);
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
		peers.on_blocks_requested(1, &vec![H256::default()]);
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
		for _ in 0..MAX_PEER_FAILURES {
			if peers.on_peer_block_failure(1) {
				peers.reset_blocks_tasks(1);
				peers.unuseful_peer(1);
			}
		}
		peers.useful_peer(1);
		assert_eq!(peers.information().active + peers.information().idle + peers.information().unuseful, 1);
	}

	#[test]
	fn peer_block_failures() {
		let mut peers = PeersTasks::default();
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

	#[test]
	fn peer_sort_peers_for_blocks() {
		let mut peers = PeersTasks::default();
		peers.on_blocks_requested(1, &vec![H256::from(1), H256::from(2)]);
		peers.on_blocks_requested(2, &vec![H256::from(3), H256::from(4)]);
		peers.on_block_received(2, &H256::from(3));
		peers.on_block_received(2, &H256::from(4));

		use std::thread;
		use std::time::Duration;
		thread::park_timeout(Duration::from_millis(50));

		peers.on_block_received(1, &H256::from(1));
		peers.on_block_received(1, &H256::from(2));

		let mut peers_for_blocks: Vec<PeerIndex> = vec![1, 2];
		peers.sort_peers_for_blocks(&mut peers_for_blocks);
		assert_eq!(peers_for_blocks[0], 2);
		assert_eq!(peers_for_blocks[1], 1);
	}
}
