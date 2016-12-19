use std::collections::{HashSet, HashMap};
use linked_hash_map::LinkedHashMap;
use primitives::hash::H256;
use types::PeerIndex;
use utils::AverageSpeedMeter;

/// Synchronization peers tasks
pub trait PeersTasks {
	/// Return all available peers
	fn all(&self) -> &HashSet<PeerIndex>;
	/// Return idle peers for headers requests
	fn idle_for_headers(&self) -> &HashSet<PeerIndex>;
	/// Return idle peers for blocks requests
	fn idle_for_blocks(&self) -> &HashSet<PeerIndex>;
	/// Mark useful peer
	fn mark_useful(&mut self, peer_index: PeerIndex);
	/// Mark unuseful peer
	fn mark_unuseful(&mut self, peer_index: PeerIndex);
	/// Reset peer blocks tasks
	fn reset_blocks_tasks(&mut self, peer_index: PeerIndex) -> Vec<H256>;
	/// When new peer is connected
	fn on_peer_connected(&mut self, peer_index: PeerIndex);
	/// When new peer is disconnected
	fn on_peer_disconnected(&mut self, peer_index: PeerIndex);
	/// When headers are received from peer
	fn on_headers_received(&mut self, peer_index: PeerIndex);
	/// When block received from peer
	fn on_block_received(&mut self, peer_index: PeerIndex, hash: &H256);
	/// When response for headers request hasn't been received for too long.
	/// Returns true if peer has been removed from useful peers list.
	fn on_headers_failure(&mut self, peer_index: PeerIndex) -> bool;
	/// When response for blocks request hasn't been received for too long.
	/// Returns true if peer has been removed from useful peers list.
	fn on_block_failure(&mut self, peer_index: PeerIndex) -> usize;
}

/// Synchronization peers tasks implementation
pub struct PeersTasksImpl {
	/// Synchronization peers stats
	stats: HashMap<PeerIndex, PeerStats>,
	/// All available peers indexes
	all: HashSet<PeerIndex>,
	/// Peers, which are available for headers requests
	idle_for_headers: HashSet<PeerIndex>,
	/// Peers, which are available for blocks requests
	idle_for_blocks: HashSet<PeerIndex>,
	/// Active headers requests
	active_headers: LinkedHashMap<PeerIndex, HeadersRequest>,
	/// Active blocks requests
	active_blocks: LinkedHashMap<PeerIndex, BlocksRequest>,
}

/// Headers request
struct HeadersRequest {
	/// Time of request
	pub time: f64,
}

/// Headers request
struct BlocksRequest {
	/// Time of request
	pub time: f64,
	/// Requested blocks
	pub blocks: LinkedHashMap<H256>,
}

/// Synchronization peer tasks statistics
#[derive(Debug, Default)]
struct PeerStats {
	/// Number of blocks requests failures
	pub blocks_failures: usize,
	/// Average block response time meter
	pub blocks_response_meter: AverageSpeedMeter,
}

impl PeersTasks for PeersTasksImpl {
	fn all(&self) -> &HashSet<PeerIndex> {
		&self.all
	}

	fn idle_for_headers(&self) -> &HashSet<PeerIndex> {
		&self.idle_for_headers
	}

	fn idle_for_blocks(&self) -> &HashSet<PeerIndex> {
		&self.idle_for_blocks
	}

	fn mark_useful(&mut self, peer_index: PeerIndex) {
		let has_active_blocks_requests = self.active_blocks.contains_key(peer_index);
		if !has_active_blocks_requests {
			self.idle_for_blocks.insert(peer_index);
		}
		if !has_active_blocks_requests && !self.active_headers.contains_key(peer_index) {
			self.idle_for_headers.insert(peer_index);
		}
	}

	fn mark_unuseful(&mut self, peer_index: PeerIndex) {
		self.idle_for_headers.remove(peer_index);
		self.idle_for_blocks.remove(peer_index);
		self.active_headers.remove(peer_index);
		self.active_blocks.remove(peer_index);
	}

	fn reset_blocks_tasks(&mut self, peer_index: PeerIndex) -> Vec<H256> {
		self.active_blocks.remove(peer_index)
			.map(|hs| hs.drain().map(|(k, _)| k).collect())
			.unwrap_or(Vec::new)
	}

	fn on_peer_connected(&mut self, peer_index: PeerIndex) {
		self.stats.insert(peer_index, PeerStats::default());
		self.all.insert(peer_index);
		self.idle_for_headers.insert(peer_index);
		self.idle_for_blocks.insert(peer_index);
	}

	fn on_peer_disconnected(&mut self, peer_index: PeerIndex) {
		self.stats.remove(peer_index);
		self.all.remove(peer_index);
		self.idle_for_headers.remove(peer_index);
		self.idle_for_blocks.remove(peer_index);
		self.active_headers.remove(peer_index);
		self.active_blocks.remove(peer_index);
	}

	fn on_headers_received(&mut self, peer_index: PeerIndex) {
		self.active_headers.remove(peer_index);
		// we only ask for new headers when peer is also not asked for blocks
		// => only insert to idle queue if no active blocks requests
		if !self.active_blocks.contains_key(peer_index) {
			self.idle_for_headers.insert(peer_index);
		}
	}

	fn on_block_received(&mut self, peer_index: PeerIndex, hash: &H256) {
		let is_last_requested_block_received = if let Some(blocks_request) = self.active_blocks.get(peer_index) {
			// if block hasn't been requested => do nothing
			if !blocks_request.remove(hash) {
				return;
			}

			blocks_request.is_empty()
		} else {
			// this peers hasn't been requested for blocks at all
			return;
		};

		// it was requested block => update block response time
		self.stats[peer_index].blocks_response_meter.checkpoint();

		// if it hasn't been last requested block => just return
		if !is_last_requested_block_received {
			return;
		}

		// mark this peer as idle for blocks request
		self.active_blocks.remove(peer_index);
		self.idle_for_blocks.insert(peer_index);
		// also mark as available for headers request if not yet
		if !self.active_headers.contains_key(peer_index) {
			self.idle_for_headers.insert(peer_index);
		}
	}

	fn on_headers_failure(&mut self, peer_index: PeerIndex) -> bool {
		// we never penalize peers for header requests failures
		self.active_headers.remove(peer_index);
		self.idle_for_headers.insert(peer_index);
	}

	fn on_block_failure(&mut self, peer_index: PeerIndex) -> usize {
		self.stats[peer_index].blocks_failures += 1;
		self.stats[peer_index].blocks_failures
	}
}
