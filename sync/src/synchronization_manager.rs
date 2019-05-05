use std::collections::HashSet;
use std::sync::{Arc, Weak};
use std::thread;
use std::time::Duration;
use parking_lot::{Mutex, Condvar};
use time::precise_time_s;
use primitives::hash::H256;
use synchronization_client_core::{ClientCore, SynchronizationClientCore};
use synchronization_executor::TaskExecutor;
use synchronization_peers_tasks::{PeersTasks, TrustLevel};
use utils::{OrphanBlocksPool, OrphanTransactionsPool};
use types::PeersRef;

/// Management interval (in ms)
const MANAGEMENT_INTERVAL_MS: u64 = 10 * 1000;
/// Response time before getting block to decrease peer score
const DEFAULT_NEW_PEER_BLOCK_FAILURE_INTERVAL_MS: u32 = 5 * 1000;
/// Response time before getting headers to decrease peer score
const DEFAULT_NEW_PEER_HEADERS_FAILURE_INTERVAL_MS: u32 = 5 * 1000;
/// Response time before getting block to decrease peer score
const DEFAULT_TRUSTED_PEER_BLOCK_FAILURE_INTERVAL_MS: u32 = 20 * 1000;
/// Response time before getting headers to decrease peer score
const DEFAULT_TRUSTED_PEER_HEADERS_FAILURE_INTERVAL_MS: u32 = 20 * 1000;
/// Unknown orphan block removal time
const DEFAULT_UNKNOWN_BLOCK_REMOVAL_TIME_MS: u32 = 20 * 60 * 1000;
/// Maximal number of orphaned blocks
const DEFAULT_UNKNOWN_BLOCKS_MAX_LEN: usize = 16;
/// Unknown orphan transaction removal time
const DEFAULT_ORPHAN_TRANSACTION_REMOVAL_TIME_MS: u32 = 10 * 60 * 1000;
/// Maximal number of orphaned transactions
const DEFAULT_ORPHAN_TRANSACTIONS_MAX_LEN: usize = 10000;

/// Synchronization management worker
pub struct ManagementWorker {
	/// Stop flag.
	is_stopping: Arc<Mutex<bool>>,
	/// Stop event.
	stopping_event: Arc<Condvar>,
	/// Verification thread.
	thread: Option<thread::JoinHandle<()>>,
}

impl ManagementWorker {
	pub fn new<T: TaskExecutor>(core: Weak<Mutex<SynchronizationClientCore<T>>>) -> Self {
		let is_stopping = Arc::new(Mutex::new(false));
		let stopping_event = Arc::new(Condvar::new());
		ManagementWorker {
			is_stopping: is_stopping.clone(),
			stopping_event: stopping_event.clone(),
			thread: Some(thread::Builder::new()
				.name("Sync management thread".to_string())
				.spawn(move || ManagementWorker::worker_proc(is_stopping, stopping_event, core))
				.expect("Error creating management thread"))
		}
	}

	fn worker_proc<T: TaskExecutor>(is_stopping: Arc<Mutex<bool>>, stopping_event: Arc<Condvar>, core: Weak<Mutex<SynchronizationClientCore<T>>>) {
		let peers_config = ManagePeersConfig::default();
		let unknown_config = ManageUnknownBlocksConfig::default();
		let orphan_config = ManageOrphanTransactionsConfig::default();

		loop {
			let mut lock = is_stopping.lock();
			if *lock {
				break;
			}

			if !stopping_event.wait_for(&mut lock, Duration::from_millis(MANAGEMENT_INTERVAL_MS)).timed_out() {
				if *lock {
					break;
				}

				// spurious wakeup?
				continue;
			}
			drop(lock);

			// if core is dropped => stop thread
			let core = match core.upgrade() {
				None => break,
				Some(core) => core,
			};

			let mut core = core.lock();
			// trace synchronization state
			core.print_synchronization_information();
			// execute management tasks if not saturated
			if core.state().is_synchronizing() || core.state().is_nearly_saturated() {
				let (blocks_to_request, blocks_to_forget) = manage_synchronization_peers_blocks(&peers_config, core.peers(), core.peers_tasks());
				core.forget_failed_blocks(&blocks_to_forget);
				core.execute_synchronization_tasks(
					if blocks_to_request.is_empty() { None } else { Some(blocks_to_request) },
					if blocks_to_forget.is_empty() { None } else { Some(blocks_to_forget) },
				);

				manage_synchronization_peers_headers(&peers_config, core.peers(), core.peers_tasks());
				manage_orphaned_transactions(&orphan_config, core.orphaned_transactions_pool());
			} else {
				// only remove orphaned blocks when not in synchronization state
				if let Some(orphans_to_remove) = manage_unknown_orphaned_blocks(&unknown_config, core.orphaned_blocks_pool()) {
					for orphan_to_remove in orphans_to_remove {
						core.chain().forget_block(&orphan_to_remove);
					}
				}
			}
		}

		trace!(target: "sync", "Stopping sync management thread");
	}
}

impl Drop for ManagementWorker {
	fn drop(&mut self) {
		if let Some(join_handle) = self.thread.take() {
			*self.is_stopping.lock() = true;
			self.stopping_event.notify_all();
			join_handle.join().expect("Clean shutdown.");
		}
	}
}


/// Peers management configuration
pub struct ManagePeersConfig {
	pub new_block_failure_interval_ms: u32,
	/// Time interval (in milliseconds) to wait headers from the peer before penalizing && reexecuting tasks
	pub new_headers_failure_interval_ms: u32,
	/// Time interval (in milliseconds) to wait block from the peer before penalizing && reexecuting tasks
	pub trusted_block_failure_interval_ms: u32,
	/// Time interval (in milliseconds) to wait headers from the peer before penalizing && reexecuting tasks
	pub trusted_headers_failure_interval_ms: u32,
}

impl Default for ManagePeersConfig {
	fn default() -> Self {
		ManagePeersConfig {
			new_block_failure_interval_ms: DEFAULT_NEW_PEER_BLOCK_FAILURE_INTERVAL_MS,
			new_headers_failure_interval_ms: DEFAULT_NEW_PEER_HEADERS_FAILURE_INTERVAL_MS,
			trusted_block_failure_interval_ms: DEFAULT_TRUSTED_PEER_BLOCK_FAILURE_INTERVAL_MS,
			trusted_headers_failure_interval_ms: DEFAULT_TRUSTED_PEER_HEADERS_FAILURE_INTERVAL_MS,
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

/// Orphan transactions management configuration
pub struct ManageOrphanTransactionsConfig {
	/// Time interval (in milliseconds) to wait before removing orphan transactions from orphan pool
	pub removal_time_ms: u32,
	/// Maximal # of unknown transactions in the orphan pool
	pub max_number: usize,
}

impl Default for ManageOrphanTransactionsConfig {
	fn default() -> Self {
		ManageOrphanTransactionsConfig {
			removal_time_ms: DEFAULT_ORPHAN_TRANSACTION_REMOVAL_TIME_MS,
			max_number: DEFAULT_ORPHAN_TRANSACTIONS_MAX_LEN,
		}
	}
}

/// Manage stalled synchronization peers blocks tasks
pub fn manage_synchronization_peers_blocks(config: &ManagePeersConfig, peers: PeersRef, peers_tasks: &mut PeersTasks) -> (Vec<H256>, Vec<H256>) {
	let mut blocks_to_request: Vec<H256> = Vec::new();
	let mut blocks_to_forget: Vec<H256> = Vec::new();
	let now = precise_time_s();

	// reset tasks for peers, which has not responded during given period
	let ordered_blocks_requests: Vec<_> = peers_tasks.ordered_blocks_requests().clone().into_iter().collect();
	for (worst_peer_index, blocks_request) in ordered_blocks_requests {
		// check if peer has not responded within given time
		let is_trusted = peers_tasks.get_peer_stats(worst_peer_index).map(|s| s.trust() == TrustLevel::Trusted).unwrap_or(false);
		let block_failure_interval = if is_trusted { config.trusted_block_failure_interval_ms } else { config.new_block_failure_interval_ms };
		let time_diff = now - blocks_request.timestamp;
		if time_diff <= block_failure_interval as f64 / 1000f64 {
			break;
		}

		// decrease score && move to the idle queue
		warn!(target: "sync", "Failed to get requested block from peer#{} in {:.2} seconds.", worst_peer_index, time_diff);
		let failed_blocks = peers_tasks.reset_blocks_tasks(worst_peer_index);

		// mark blocks as failed
		let (normal_blocks, failed_blocks) = peers_tasks.on_blocks_failure(failed_blocks);
		blocks_to_request.extend(normal_blocks);
		blocks_to_forget.extend(failed_blocks);

		// if peer failed many times => forget it
		if peers_tasks.on_peer_block_failure(worst_peer_index) {
			warn!(target: "sync", "Too many failures for peer#{}. Excluding from synchronization.", worst_peer_index);
			peers_tasks.unuseful_peer(worst_peer_index);
			peers.misbehaving(worst_peer_index, &format!("Too many failures."));
		}
	}

	(blocks_to_request, blocks_to_forget)
}

/// Manage stalled synchronization peers headers tasks
pub fn manage_synchronization_peers_headers(config: &ManagePeersConfig, peers: PeersRef, peers_tasks: &mut PeersTasks) {
	let now = precise_time_s();
	// reset tasks for peers, which has not responded during given period
	let ordered_headers_requests: Vec<_> = peers_tasks.ordered_headers_requests().clone().into_iter().collect();
	for (worst_peer_index, headers_request) in ordered_headers_requests {
		// check if peer has not responded within given time
		let is_trusted = peers_tasks.get_peer_stats(worst_peer_index).map(|s| s.trust() == TrustLevel::Trusted).unwrap_or(false);
		let headers_failure_interval = if is_trusted { config.trusted_headers_failure_interval_ms } else { config.new_headers_failure_interval_ms };
		let time_diff = now - headers_request.timestamp;
		if time_diff <= headers_failure_interval as f64 / 1000f64 {
			break;
		}

		// do not penalize peer if it has pending blocks tasks
		if peers_tasks.get_blocks_tasks(worst_peer_index).map(|t| !t.is_empty()).unwrap_or(false) {
			continue;
		}

		// if peer failed many times => forget it
		if peers_tasks.on_peer_headers_failure(worst_peer_index) {
			warn!(target: "sync", "Too many header failures for peer#{}. Excluding from synchronization.", worst_peer_index);
			peers.misbehaving(worst_peer_index, &format!("Too many header failures."));
		}
	}
}

/// Manage unknown orphaned blocks
pub fn manage_unknown_orphaned_blocks(config: &ManageUnknownBlocksConfig, orphaned_blocks_pool: &mut OrphanBlocksPool) -> Option<Vec<H256>> {
	let unknown_to_remove = {
		let unknown_blocks = orphaned_blocks_pool.unknown_blocks();
		let mut unknown_to_remove: HashSet<H256> = HashSet::new();
		let mut remove_num = if unknown_blocks.len() > config.max_number { unknown_blocks.len() - config.max_number } else { 0 };
		let now = precise_time_s();
		for (hash, time) in unknown_blocks {
			// remove oldest blocks if there are more unknown blocks that we can hold in memory
			if remove_num > 0 {
				unknown_to_remove.insert(hash.clone());
				remove_num -= 1;
				continue;
			}

			// check if block is unknown for too long
			let time_diff = now - time;
			if time_diff <= config.removal_time_ms as f64 / 1000f64 {
				break;
			}
			unknown_to_remove.insert(hash.clone());
		}

		unknown_to_remove
	};

	// remove unknown blocks
	let unknown_to_remove: Vec<H256> = orphaned_blocks_pool.remove_blocks(&unknown_to_remove).into_iter()
		.map(|b| b.header.hash)
		.collect();

	if unknown_to_remove.is_empty() { None } else { Some(unknown_to_remove) }
}

/// Manage orphaned transactions
pub fn manage_orphaned_transactions(config: &ManageOrphanTransactionsConfig, orphaned_transactions_pool: &mut OrphanTransactionsPool) -> Option<Vec<H256>> {
	let orphans_to_remove = {
		let unknown_transactions = orphaned_transactions_pool.transactions();
		let mut orphans_to_remove: Vec<H256> = Vec::new();
		let mut remove_num = if unknown_transactions.len() > config.max_number { unknown_transactions.len() - config.max_number } else { 0 };
		let now = precise_time_s();
		for (hash, orphan_tx) in unknown_transactions {
			// remove oldest transactions if there are more unknown transactions that we can hold in memory
			if remove_num > 0 {
				orphans_to_remove.push(hash.clone());
				remove_num -= 1;
				continue;
			}

			// check if transaction is unknown for too long
			let time_diff = now - orphan_tx.insertion_time;
			if time_diff <= config.removal_time_ms as f64 / 1000f64 {
				break;
			}
			orphans_to_remove.push(hash.clone());
		}

		orphans_to_remove
	};

	// remove unknown transactions
	let orphans_to_remove: Vec<H256> = orphaned_transactions_pool.remove_transactions(&orphans_to_remove).into_iter()
		.map(|t| t.hash)
		.collect();

	if orphans_to_remove.is_empty() { None } else { Some(orphans_to_remove) }
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use std::collections::HashSet;
	use primitives::hash::H256;
	use synchronization_peers::PeersImpl;
	use synchronization_peers_tasks::{PeersTasks, TrustLevel};
	use super::{ManagePeersConfig, ManageUnknownBlocksConfig, ManageOrphanTransactionsConfig, manage_synchronization_peers_blocks,
		manage_unknown_orphaned_blocks, manage_orphaned_transactions};
	use utils::{OrphanBlocksPool, OrphanTransactionsPool};

	#[test]
	fn manage_good_peer() {
		let config = ManagePeersConfig { new_block_failure_interval_ms: 1000, ..Default::default() };
		let mut peers = PeersTasks::default();
		peers.on_blocks_requested(1, &vec![H256::from(0), H256::from(1)]);
		peers.on_block_received(1, &H256::from(0));
		assert_eq!(manage_synchronization_peers_blocks(&config, Arc::new(PeersImpl::default()), &mut peers), (vec![], vec![]));
		assert_eq!(peers.idle_peers_for_blocks().len(), 0);
	}

	#[test]
	fn manage_bad_peers() {
		use std::thread::sleep;
		use std::time::Duration;
		let config = ManagePeersConfig { trusted_block_failure_interval_ms: 0, ..Default::default() };
		let mut peers = PeersTasks::default();
		peers.on_blocks_requested(1, &vec![H256::from(0)]);
		peers.on_blocks_requested(2, &vec![H256::from(1)]);
		peers.get_peer_stats_mut(1).unwrap().set_trust(TrustLevel::Trusted);
		peers.get_peer_stats_mut(2).unwrap().set_trust(TrustLevel::Trusted);
		sleep(Duration::from_millis(1));

		let managed_tasks = manage_synchronization_peers_blocks(&config, Arc::new(PeersImpl::default()), &mut peers).0;
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
		let mut pool = OrphanBlocksPool::new();
		let block = test_data::genesis();
		pool.insert_unknown_block(block.into());
		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut pool), None);
		assert_eq!(pool.len(), 1);
	}

	#[test]
	fn manage_unknown_blocks_by_time() {
		use std::thread::sleep;
		use std::time::Duration;
		let config = ManageUnknownBlocksConfig { removal_time_ms: 0, max_number: 100 };
		let mut pool = OrphanBlocksPool::new();
		let block = test_data::genesis();
		let block_hash = block.hash();
		pool.insert_unknown_block(block.into());
		sleep(Duration::from_millis(1));

		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut pool), Some(vec![block_hash]));
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn manage_unknown_blocks_by_max_number() {
		let config = ManageUnknownBlocksConfig { removal_time_ms: 100, max_number: 1 };
		let mut pool = OrphanBlocksPool::new();
		let block1 = test_data::genesis();
		let block1_hash = block1.hash();
		let block2 = test_data::block_h2();
		pool.insert_unknown_block(block1.into());
		pool.insert_unknown_block(block2.into());
		assert_eq!(manage_unknown_orphaned_blocks(&config, &mut pool), Some(vec![block1_hash]));
		assert_eq!(pool.len(), 1);
	}

	#[test]
	fn manage_orphan_transactions_good() {
		let config = ManageOrphanTransactionsConfig { removal_time_ms: 1000, max_number: 100 };
		let mut pool = OrphanTransactionsPool::new();
		let transaction = test_data::block_h170().transactions[1].clone();
		let unknown_inputs: HashSet<H256> = transaction.inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		pool.insert(transaction.into(), unknown_inputs);
		assert_eq!(manage_orphaned_transactions(&config, &mut pool), None);
		assert_eq!(pool.len(), 1);
	}

	#[test]
	fn manage_orphan_transactions_by_time() {
		use std::thread::sleep;
		use std::time::Duration;
		let config = ManageOrphanTransactionsConfig { removal_time_ms: 0, max_number: 100 };
		let mut pool = OrphanTransactionsPool::new();
		let transaction = test_data::block_h170().transactions[1].clone();
		let unknown_inputs: HashSet<H256> = transaction.inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let transaction_hash = transaction.hash();
		pool.insert(transaction.into(), unknown_inputs);
		sleep(Duration::from_millis(1));

		assert_eq!(manage_orphaned_transactions(&config, &mut pool), Some(vec![transaction_hash]));
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn manage_orphan_transactions_by_max_number() {
		let config = ManageOrphanTransactionsConfig { removal_time_ms: 100, max_number: 1 };
		let mut pool = OrphanTransactionsPool::new();
		let transaction1 = test_data::block_h170().transactions[1].clone();
		let unknown_inputs1: HashSet<H256> = transaction1.inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let transaction1_hash = transaction1.hash();
		let transaction2 = test_data::block_h182().transactions[1].clone();
		let unknown_inputs2: HashSet<H256> = transaction2.inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		pool.insert(transaction1.into(), unknown_inputs1);
		pool.insert(transaction2.into(), unknown_inputs2);
		assert_eq!(manage_orphaned_transactions(&config, &mut pool), Some(vec![transaction1_hash]));
		assert_eq!(pool.len(), 1);
	}
}
