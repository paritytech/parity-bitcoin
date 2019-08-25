use std::sync::Arc;
use parking_lot::Mutex;
use chain::{IndexedTransaction, IndexedBlock, IndexedBlockHeader};
use message::types;
use synchronization_executor::TaskExecutor;
use synchronization_verifier::{Verifier, TransactionVerificationSink};
use synchronization_client_core::{ClientCore, SynchronizationClientCore};
use types::{PeerIndex, ClientCoreRef, SynchronizationStateRef, EmptyBoxFuture, SyncListenerRef};

#[cfg_attr(feature="cargo-clippy", allow(doc_markdown))]
///! TODO: update with headers-first corrections
///!
///! Blocks synchronization process:
///!
///! When new peer is connected:
///! 1) send `getheaders` message with full block locator hashes (see `LocalNode`)
///!
///! on_new_blocks_headers: When `headers` message is received from peer:
///! 1) queue_intersection = intersect(queue, inventory)
///! 2) if !queue_intersection.is_empty(): ===> responded with blocks within sync window
///! 2.1) remember peer as useful
///! 2.2) inventory_rest = inventory - queue_intersection
///! 2.3) if inventory_rest.is_empty(): ===> no new unknown blocks in inventory
///! 2.3.1) stop (2.3)
///! 2.4) if !inventory_rest.is_empty(): ===> has new unknown blocks in inventory
///! 2.4.1) queue_rest = queue after intersection
///! 2.4.2) if queue_rest.is_empty(): ===> has new unknown blocks in inventory, no fork
///! 2.4.2.1) scheduled_blocks.append(inventory_rest)
///! 2.4.2.2) stop (2.4.2)
///! 2.4.3) if !queue_rest.is_empty(): ===> has new unknown blocks in inventory, fork
///! 2.4.3.1) scheduled_blocks.append(inventory_rest)
///! 2.4.3.2) stop (2.4.3)
///! 2.4.3) stop (2.4)
///! 2.5) stop (2)
///! 3) if queue_intersection.is_empty(): ===> responded with out-of-sync-window blocks
///! 3.1) last_known_block = inventory.last(b => b.is_known())
///! 3.2) if last_known_block == None: ===> we know nothing about these blocks & we haven't asked for these
///! 3.2.1) if !synchronizing => remember peer as useful + ask for blocks
///! 3.2.1) if synchronizing => peer will be excluded later by management thread
///! 3.2.2) stop (3.2)
///! 3.3) if last_known_block == last(inventory): ===> responded with all-known-blocks
///! 3.3.1) if syncing, remember peer as useful (possibly had failures before && have been excluded from sync)
///! 3.3.2) stop (3.3)
///! 3.4) if last_known_block in the middle of inventory: ===> responded with forked blocks
///! 3.4.1) remember peer as useful
///! 3.4.2) inventory_rest = inventory after last_known_block
///! 3.4.3) scheduled_blocks.append(inventory_rest)
///! 3.4.4) stop (3.4)
///! 3.5) stop (3)
///!
///! on_peer_block: After receiving `block` message:
///! 1) if block_state(block) in (Verifying, Stored): ===> late delivery
///! 1.1) remember peer as useful
///! 1.2) stop (1)
///! 2) if block_state(block) in (Scheduled, Requested): ===> future/on-time delivery
///! 2.1) remember peer as useful
///! 2.2) if block_state(block.parent) in (Verifying, Stored): ===> we can proceed with verification
///! 2.2.1) remove block from current queue (Verifying || Stored)
///! 2.2.2) append block to the verification queue
///! 2.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 2.2.4) try to verify orphan blocks
///! 2.2.5) stop (2.2)
///! 2.3) if block_state(block.parent) in (Requested, Scheduled): ===> we have found an orphan block
///! 2.3.1) remove block from current queue (Verifying || Stored)
///! 2.3.2) append block to the orphans
///! 2.3.3) stop (2.3)
///! 2.4) if block_state(block.parent) == Unknown: ===> bad block found
///! 2.4.1) remove block from current queue (Verifying || Stored)
///! 2.4.2) stop (2.4)
///! 2.5) stop (2)
///! 3) if block_state(block) == Unknown: ===> maybe we are on-top of chain && new block is announced?
///! 3.1) if block_state(block.parent_hash) == Unknown: ===> we do not know parent
///! 3.1.1) ignore this block
///! 3.1.2) stop (3.1)
///! 3.2) if block_state(block.parent_hash) in (Verifying, Stored): ===> fork found, can verify
///! 3.2.1) ask peer for best inventory (after this block)
///! 3.2.2) append block to verifying queue
///! 3.2.3) queue verification().and_then(on_block_verification_success).or_else(on_block_verification_error)
///! 3.2.4) stop (3.2)
///! 3.3) if block_state(block.parent_hash) in (Requested, Scheduled): ===> fork found, add as orphan
///! 3.3.1) ask peer for best inventory (after this block)
///! 3.3.2) append block to orphan
///! 3.3.3) stop (3.3)
///! 3.4) stop (2)
///! + if no blocks left in scheduled + requested queue => we are saturated => ask all peers for inventory & forget
///!
///! execute_synchronization_tasks: After receiving `headers`/`inventory` message OR receiving `block` message OR when management thread schedules tasks:
///! 1) if there are blocks in `scheduled` queue AND we can fit more blocks into memory: ===> ask for blocks
///! 1.1) select idle peers
///! 1.2) for each idle peer: query chunk of blocks from `scheduled` queue
///! 1.3) move requested blocks from `scheduled` to `requested` queue
///! 1.4) mark idle peers as active
///! 1.5) stop (1)
///! 2) if `scheduled` queue is not yet saturated: ===> ask for new blocks hashes
///! 2.1) for each idle peer: send shortened `getblocks` message
///! 2.2) 'forget' idle peers => they will be added again if respond with inventory
///! 2.3) stop (2)
///!
///! manage_synchronization_peers: When management thread awakes:
///! 1) for peer in active_peers.where(p => now() - p.last_request_time() > failure_interval):
///! 1.1) return all peer' tasks to the tasks pool
///! 1.2) increase # of failures for this peer
///! 1.3) if # of failures > max_failures: ===> super-bad peer
///! 1.3.1) forget peer
///! 1.3.3) stop (1.3)
///! 1.4) if # of failures <= max_failures: ===> bad peer
///! 1.4.1) move peer to idle pool
///! 1.4.2) stop (1.4)
///! 2) schedule tasks from pool (if any)
///!
///! on_block_verification_success: When verification completes scuccessfully:
///! 1) if block_state(block) != Verifying: ===> parent verification failed
///! 1.1) stop (1)
///! 2) remove from verifying queue
///! 3) insert to the db
///!
///! on_block_verification_error: When verification completes with an error:
///! 1) remove block from verification queue
///! 2) remove all known children from all queues [so that new `block` messages will be ignored in on_peer_block.3.1.1]
///!

/// Synchronization client trait
pub trait Client : Send + Sync + 'static {
	fn on_connect(&self, peer_index: PeerIndex);
	fn on_disconnect(&self, peer_index: PeerIndex);
	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv);
	fn on_headers(&self, peer_index: PeerIndex, headers: Vec<IndexedBlockHeader>);
	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock);
	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction);
	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound);
	fn after_peer_nearly_blocks_verified(&self, peer_index: PeerIndex, future: EmptyBoxFuture);
	fn accept_transaction(&self, transaction: IndexedTransaction, sink: Box<dyn TransactionVerificationSink>) -> Result<(), String>;
	fn install_sync_listener(&self, listener: SyncListenerRef);
}

/// Synchronization client facade
pub struct SynchronizationClient<T: TaskExecutor, U: Verifier> {
	/// Verification mutex
	verification_lock: Mutex<()>,
	/// Shared client state
	shared_state: SynchronizationStateRef,
	/// Client core
	core: ClientCoreRef<SynchronizationClientCore<T>>,
	/// Verifier
	verifier: U,
}

impl<T, U> Client for SynchronizationClient<T, U> where T: TaskExecutor, U: Verifier {
	fn on_connect(&self, peer_index: PeerIndex) {
		self.core.lock().on_connect(peer_index);
	}

	fn on_disconnect(&self, peer_index: PeerIndex) {
		self.core.lock().on_disconnect(peer_index);
	}

	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv) {
		self.core.lock().on_inventory(peer_index, message);
	}

	fn on_headers(&self, peer_index: PeerIndex, headers: Vec<IndexedBlockHeader>) {
		self.core.lock().on_headers(peer_index, headers);
	}

	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock) {
		// block can became:
		// ignored, unknown, orphaned => no verification should occur
		// on-time => this block + all dependent orphaned should be verified
		{
			// verification tasks must be scheduled in the same order as they were built in on_block
			// => here we use verification_lock for this
			let _verification_lock = self.verification_lock.lock();
			let blocks_to_verify = self.core.lock().on_block(peer_index, block);

			// verify blocks
			if let Some(mut blocks_to_verify) = blocks_to_verify {
				while let Some(block) = blocks_to_verify.pop_front() {
					self.verifier.verify_block(block);
				}
			}
		}

		// in case if verification was synchronous
		// => try to switch to saturated state OR execute sync tasks
		let mut client = self.core.lock();
		if !client.try_switch_to_saturated_state() {
			client.execute_synchronization_tasks(None, None);
		}
	}

	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction) {
		// block can became:
		// ignored, orphaned => no verification should occur
		// on-time => this transaction + all dependent orphaned should be verified
		let transactions_to_verify = self.core.lock().on_transaction(peer_index, transaction);

		if let Some(mut transactions_to_verify) = transactions_to_verify {
			// it is not actual height of block this transaction will be included to
			// => it possibly will be invalid if included in later blocks
			// => mined block can be rejected
			// => we should verify blocks we mine
			let next_block_height = self.shared_state.best_storage_block_height() + 1;
			while let Some(tx) = transactions_to_verify.pop_front() {
				self.verifier.verify_transaction(next_block_height, tx);
			}
		}
	}

	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound) {
		self.core.lock().on_notfound(peer_index, message);
	}

	fn after_peer_nearly_blocks_verified(&self, peer_index: PeerIndex, future: EmptyBoxFuture) {
		self.core.lock().after_peer_nearly_blocks_verified(peer_index, future);
	}

	fn accept_transaction(&self, transaction: IndexedTransaction, sink: Box<dyn TransactionVerificationSink>) -> Result<(), String> {
		let mut transactions_to_verify = try!(self.core.lock().accept_transaction(transaction, sink));

		let next_block_height = self.shared_state.best_storage_block_height() + 1;
		while let Some(tx) = transactions_to_verify.pop_front() {
			self.verifier.verify_transaction(next_block_height, tx);
		}
		Ok(())
	}

	fn install_sync_listener(&self, listener: SyncListenerRef) {
		self.core.lock().install_sync_listener(listener);
	}
}

impl<T, U> SynchronizationClient<T, U> where T: TaskExecutor, U: Verifier {
	/// Create new synchronization client
	pub fn new(shared_state: SynchronizationStateRef, core: ClientCoreRef<SynchronizationClientCore<T>>, verifier: U) -> Arc<Self> {
		Arc::new(SynchronizationClient {
			verification_lock: Mutex::new(()),
			shared_state: shared_state,
			core: core,
			verifier: verifier,
		})
	}
}
