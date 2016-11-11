use time::precise_time_s;
use linked_hash_map::LinkedHashMap;
use synchronization_peers::Peers;
use primitives::hash::H256;

/// Management interval (in ms)
pub const MANAGEMENT_INTERVAL_MS: u64 = 10 * 1000;
/// Response time to decrease peer score
const PEER_FAILURE_INTERVAL_S: f64 = 5f64;
/// Unknown orphan block removal time
const UNKNOWN_BLOCK_REMOVAL_TIME_S: f64 = 20f64 * 60f64;
/// Maximal number of orphaned blocks
const UNKNOWN_BLOCKS_MAX_LEN: usize = 16;

/// Manage stalled synchronization peers tasks
pub fn manage_synchronization_peers(peers: &mut Peers) -> Option<Vec<H256>> {
	let mut blocks_to_request: Vec<H256> = Vec::new();
	let now = precise_time_s();
	// reset tasks for peers, which has not responded during given period
	for (worst_peer_index, worst_peer_time) in peers.worst_peers() {
		// check if peer has not responded within given time
		let time_diff = worst_peer_time - now;
		if time_diff <= PEER_FAILURE_INTERVAL_S {
			break;
		}

		// decrease score && move to the idle queue
		trace!(target: "sync", "Failed to get response from peer#{} in {} seconds", worst_peer_index, time_diff);
		let peer_tasks = peers.reset_tasks(worst_peer_index);
		blocks_to_request.extend(peer_tasks);

		// if peer failed many times => forget it
		if peers.on_peer_failure(worst_peer_index) {
			trace!(target: "sync", "Too many failures for peer#{}. Excluding from synchronization", worst_peer_index);
		}
	}

	if blocks_to_request.is_empty() { None } else { Some(blocks_to_request) }
}

/// Manage unknown orphaned blocks
pub fn manage_unknown_orphaned_blocks(unknown_blocks: &mut LinkedHashMap<H256, f64>) -> Option<Vec<H256>> {
	let mut unknown_to_remove: Vec<H256> = Vec::new();
	let mut remove_num = if unknown_blocks.len() > UNKNOWN_BLOCKS_MAX_LEN { UNKNOWN_BLOCKS_MAX_LEN - unknown_blocks.len() } else { 0 };
	let now = precise_time_s();
	for (hash, time) in unknown_blocks.iter() {
		// remove oldest blocks if there are more unknown blocks that we can hold in memory
		if remove_num > 0 {
			unknown_to_remove.push(hash.clone());
			remove_num -= 1;
			continue;
		}

		// check if block is unknown for too long
		let time_diff = time - now;
		if time_diff <= UNKNOWN_BLOCK_REMOVAL_TIME_S {
			break;
		}
		unknown_to_remove.push(hash.clone());
	}

	// remove unknown blocks
	for unknown_block in unknown_to_remove.iter() {
		unknown_blocks.remove(unknown_block);
	}

	if unknown_to_remove.is_empty() { None } else { Some(unknown_to_remove) }
}
