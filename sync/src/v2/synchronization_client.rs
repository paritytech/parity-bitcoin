use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use synchronization_client_core::ClientCore;

/// Shared client state
pub struct ClientState {
	/// Is synchronizing flag
	is_synchronizing: Arc<AtomicBool>,
}

/// Synchronization client trait
pub trait Client {
	/// Are we currently synchronizing
	fn is_synchronizing(&self) -> bool;
	/// Called upon receiving inventory message
	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv);
	/// Called upon receiving headers message
	fn on_headers(&self, peer_index: PeerIndex, message: types::Headers);
	/// Called upon receiving notfound message
	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound);
	/// Called upon receiving block message
	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock) -> Promise;
	/// Called upon receiving transaction message
	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction);
	/// Called when connection is closed
	fn on_disconnect(&self, peer_index: PeerIndex);
}

/// Synchronization client implementation
pub struct ClientImpl {
	/// State reference
	state: ClientStateRef,
	/// Core reference
	core: ClientCoreRef,
}

impl Sync for ClientState {
}

impl ClientState {
	fn is_synchronizing(&self) -> bool {
		self.state.load(Ordering::Relaxed)
	}
}

impl Client for ClientImpl {
	fn is_synchronizing(&self) -> bool {
		self.state.is_synchronizing();
	}

	fn on_inventory(&self, peer_index: PeerIndex, message: types::Inv) {
		self.core.lock().on_inventory(peer_index, message);
	}

	fn on_headers(&self, peer_index: PeerIndex, message: types::Headers) {
		self.core.lock().on_headers(peer_index, message);
	}

	fn on_notfound(&self, peer_index: PeerIndex, message: types::NotFound) {
		self.core.lock().on_notfound(peer_index, message);
	}

	fn on_block(&self, peer_index: PeerIndex, block: IndexedBlock) -> Promise {
		if let Some(verification_tasks) = self.core.lock().on_transaction(peer_index, transaction) {
			while let Some(verification_task) = verification_tasks.pop_front() {
				self.verifier.execute(verification_task);
			}
		}

		// try to switch to saturated state OR execute sync tasks
		let mut core = self.core.lock();
		if !core.try_switch_to_saturated_state() {
			core.execute_synchronization_tasks(None, None);
		}
	}

	fn on_transaction(&self, peer_index: PeerIndex, transaction: IndexedTransaction) {
		if let Some(verification_tasks) = self.client.lock().on_transaction(peer_index, transaction) {
			while let Some(verification_task) = verification_tasks.pop_front() {
				self.verifier.execute(verification_task);
			}
		}
	}

	fn on_disconnect(&self, peer_index: PeerIndex) {
		self.client.lock().on_disconnect(peer_index);
	}
}
