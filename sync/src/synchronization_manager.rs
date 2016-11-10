use time::precise_time_s;
use synchronization_peers::Peers;
use primitives::hash::H256;

/// Management interval (in ms)
pub const MANAGEMENT_INTERVAL_MS: u64 = 10 * 1000;
/// Response time to decrease peer score
const FAILURE_INTERVAL_S: f64 = 5f64;

/// Management worker
pub fn manage_synchronization_peers(peers: &mut Peers) -> Option<Vec<H256>> {
	let mut blocks_to_request: Vec<H256> = Vec::new();
	// reset tasks for peers, which has not responded during given period
	for (worst_peer_index, worst_peer_time) in peers.worst_peers() {
		// check if peer has not responded within given time
		let time_diff = worst_peer_time - precise_time_s();
		if time_diff <= FAILURE_INTERVAL_S {
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
