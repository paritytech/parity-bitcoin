use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use parking_lot::RwLock;
use net::{Connection, Channel};
use PeerId;

pub struct Connections {
	/// Incremental peer counter.
	peer_counter: AtomicUsize,
	/// All open connections.
	channels: RwLock<HashMap<PeerId, Arc<Channel>>>,
}

impl Connections {
	pub fn new() -> Self {
		Connections {
			peer_counter: AtomicUsize::default(),
			channels: RwLock::default(),
		}
	}

	/// Returns safe (nonblocking) copy of channels.
	pub fn channels(&self) -> HashMap<PeerId, Arc<Channel>> {
		self.channels.read().clone()
	}

	/// Returns number of connections.
	pub fn count(&self) -> usize {
		self.channels.read().len()
	}

	/// Stores new channel.
	pub fn store(&self, connection: Connection) {
		let id = self.peer_counter.fetch_add(1, Ordering::AcqRel);
		self.channels.write().insert(id, Arc::new(Channel::new(connection)));
	}

	/// Removes channel with given id.
	pub fn remove(&self, id: PeerId) {
		self.channels.write().remove(&id);
	}
}
