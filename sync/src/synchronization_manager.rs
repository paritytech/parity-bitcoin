use time::precise_time_s;
use linked_hash_map::LinkedHashMap;
use synchronization_peers::Peers;
use primitives::hash::H256;

/// Management interval (in ms)
pub const MANAGEMENT_INTERVAL_MS: u64 = 10 * 1000;
/// Response time before getting block to decrease peer score
const DEFAULT_PEER_BLOCK_FAILURE_INTERVAL_MS: u32 = 5 * 1000;
/// Response time before getting inventory to decrease peer score
const DEFAULT_PEER_INVENTORY_FAILURE_INTERVAL_MS: u32 = 5 * 1000;
/// Unknown orphan block removal time
const DEFAULT_UNKNOWN_BLOCK_REMOVAL_TIME_MS: u32 = 20 * 60 * 1000;
/// Maximal number of orphaned blocks
const DEFAULT_UNKNOWN_BLOCKS_MAX_LEN: usize = 16;

/// Peers management configuration
pub struct ManagePeersConfig {
	/// Time interval (in milliseconds) to wait block from the peer before penalizing && reexecuting tasks
	pub block_failure_interval_ms: u32,
	/// Time interval (in milliseconds) to wait inventory from the peer before penalizing && reexecuting tasks
	pub inventory_failure_interval_ms: u32,
}

impl Default for ManagePeersConfig {
	fn default() -> Self {
		ManagePeersConfig {
			block_failure_interval_ms: DEFAULT_PEER_BLOCK_FAILURE_INTERVAL_MS,
			inventory_failure_interval_ms: DEFAULT_PEER_INVENTORY_FAILURE_INTERVAL_MS,
		}
	}
}

/// Unknown blocks management configuration
pub struct ManageUnknownBlocksConfig {
	/// Time interval (in milliseconds) to wait before removing unknown blocks from in-memory pool
	pub removal_time_ms: u32,
	/// Maximal # of unknown blocks in the in-memory pool
	pub max_number: usize,
}

impl Default for ManageUnknownBlocksConfig {
	fn default() -> Self {
		ManageUnknownBlocksConfig {
			removal_time_ms: DEFAULT_UNKNOWN_BLOCK_REMOVAL_TIME_MS,
			max_number: DEFAULT_UNKNOWN_BLOCKS_MAX_LEN,
		}
	}
}

/// Manage stalled synchronization peers blocks tasks
pub fn manage_synchronization_peers_blocks(config: &ManagePeersConfig, peers: &mut Peers) -> Option<Vec<H256>> {
	let mut blocks_to_request: Vec<H256> = Vec::new();
	let now = precise_time_s();
	// reset tasks for peers, which has not responded during given period
	for (worst_peer_index, worst_peer_time) in peers.ordered_blocks_requests() {
		// check if peer has not responded within given time
		let time_diff = now - worst_peer_time;
		if time_diff <= config.block_failure_interval_ms as f64 / 1000f64 {
			break;
		}

		// decrease score && move to the idle queue
		warn!(target: "sync", "Failed to get requested block from peer#{} in {} seconds", worst_peer_index, time_diff);
		let peer_tasks = peers.reset_blocks_tasks(worst_peer_index);
		blocks_to_request.extend(peer_tasks);

		// if peer failed many times => forget it
		if peers.on_peer_block_failure(worst_peer_index) {
			warn!(target: "sync", "Too many failures for peer#{}. Excluding from synchronization", worst_peer_index);
		}
	}

	if blocks_to_request.is_empty() { None } else { Some(blocks_to_request) }
}

/// Manage stalled synchronization peers inventory tasks
pub fn manage_synchronization_peers_inventory(config: &ManagePeersConfig, peers: &mut Peers) {
	let now = precise_time_s();
	// reset tasks for peers, which has not responded during given period
	for (worst_peer_index, worst_peer_time) in peers.ordered_inventory_requests() {
		// check if peer has not responded within given time
		let time_diff = now - worst_peer_time;
		if time_diff <= config.inventory_failure_interval_ms as f64 / 1000f64 {
			break;
		}

		peers.on_peer_inventory_failure(worst_peer_index);
	}
}

/// Manage unknown orphaned blocks
pub fn manage_unknown_orphaned_blocks(config: &ManageUnknownBlocksConfig, unknown_blocks: &mut LinkedHashMap<H256, f64>) -> Option<Vec<H256>> {
	let mut unknown_to_remove: Vec<H256> = Vec::new();
	let mut remove_num = if unknown_blocks.len() > config.max_number { unknown_blocks.len() - config.max_number } else { 0 };
	let now = precise_time_s();
	for (hash, time) in unknown_blocks.iter() {
		// remove oldest blocks if there are more unknown blocks that we can hold in memory
		if remove_num > 0 {
			unknown_to_remove.push(hash.clone());
			remove_num -= 1;
			continue;
		}

		// check if block is unknown for too long
		let time_diff = now - time;
		if time_diff <= config.removal_time_ms as f64 / 1000f64 {
			break;
		}
		unknown_to_remove.push(hash.clone());
	}

	// remove unknown blocks
	for unknown_block in &unknown_to_remove {
		unknown_blocks.remove(unknown_block);
	}

	if unknown_to_remove.is_empty() { None } else { Some(unknown_to_remove) }
}

#[cfg(test)]
mod tests {
	use time::precise_time_s;
	use linked_hash_map::LinkedHashMap;
	use super::{ManagePeersConfig, ManageUnknownBlocksConfig, manage_synchronization_peers_blocks, manage_unknown_orphaned_blocks};
	use synchronization_peers::Peers;
	use primitives::hash::H256;

	#[test]
	fn manage_good_peer() {
		let config = ManagePeersConfig { block_failure_interval_ms: 1000, ..Default::default() };
		let mut peers = Peers::new();
		peers.on_blocks_requested(1, &vec![H256::from(0), H256::from(1)]);
		peers.on_block_received(1, &H256::from(0));
		assert_eq!(manage_synchronization_peers_blocks(&config, &mut peers), None);
		assert_eq!(peers.idle_peers_for_blocks(), vec![]);
	}

	#[test]
	fn manage_bad_peers() {
		use std::thread::sleep;
		use std::time::Duration;
		let config = ManagePeersConfig { block_failure_interval_ms: 0, ..Default::default() };
		let mut peers = Peers::new();
		peers.on_blocks_requested(1, &vec![H256::from(0)]);
		peers.on_blocks_requested(2, &vec![H256::from(1)]);
		sleep(Duration::from_millis(1));

		let managed_tasks = manage_synchronization_peers_blocks(&config, &mut peers).expect("managed tasks");
		assert!(managed_tasks.contains(&H256::from(0)));
		assert!(managed_tasks.contains(&H256::from(1)));
		let idle_peers = peers.idle_peers_for_blocks();
		assert_eq!(2, idle_peers.len());
		assert!(idle_peers.contains(&1));
		assert!(idle_peers.contains(&2));
	}

	#[test]
	fn manage_unknown_blocks_good() {
		let config = ManageUnknownBlocksConfig { removal_time_ms: 1000, max_number: 100 };
		let mut unknown_blocks: LinkedHashMap<H256, f64> = LinkedHashMap::new();
		unknown_blocks.insert(H256::from(0), precise_time_s());
		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut unknown_blocks), None);
		assert_eq!(unknown_blocks.len(), 1);
	}

	#[test]
	fn manage_unknown_blocks_by_time() {
		use std::thread::sleep;
		use std::time::Duration;
		let config = ManageUnknownBlocksConfig { removal_time_ms: 0, max_number: 100 };
		let mut unknown_blocks: LinkedHashMap<H256, f64> = LinkedHashMap::new();
		unknown_blocks.insert(H256::from(0), precise_time_s());
		sleep(Duration::from_millis(1));

		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut unknown_blocks), Some(vec![H256::from(0)]));
		assert_eq!(unknown_blocks.len(), 0);
	}

	#[test]
	fn manage_unknown_blocks_by_max_number() {
		let config = ManageUnknownBlocksConfig { removal_time_ms: 100, max_number: 1 };
		let mut unknown_blocks: LinkedHashMap<H256, f64> = LinkedHashMap::new();
		unknown_blocks.insert(H256::from(0), precise_time_s());
		unknown_blocks.insert(H256::from(1), precise_time_s());
		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut unknown_blocks), Some(vec![H256::from(0)]));
		assert_eq!(unknown_blocks.len(), 1);
	}
}
