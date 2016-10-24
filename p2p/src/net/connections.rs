use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use parking_lot::RwLock;
use net::{Connection, Channel};
use p2p::Context;
use session::{SessionFactory};
use PeerId;

#[derive(Default)]
pub struct Connections {
	/// Incremental peer counter.
	peer_counter: AtomicUsize,
	/// All open connections.
	channels: RwLock<HashMap<PeerId, Arc<Channel>>>,
}

impl Connections {
	/// Returns channel with given peer id.
	pub fn channel(&self, id: PeerId) -> Option<Arc<Channel>> {
		self.channels.read().get(&id).cloned()
	}

	/// Returns safe (nonblocking) copy of channels.
	pub fn _channels(&self) -> HashMap<PeerId, Arc<Channel>> {
		self.channels.read().clone()
	}

	/// Returns number of connections.
	pub fn _count(&self) -> usize {
		self.channels.read().len()
	}

	/// Stores new channel.
	/// Returnes a shared pointer to it.
	pub fn store<T>(&self, context: Arc<Context>, connection: Connection) -> Arc<Channel> where T: SessionFactory {
		let id = self.peer_counter.fetch_add(1, Ordering::AcqRel);
		let session = T::new_session(context, id);
		let channel = Arc::new(Channel::new(connection, id, session));
		self.channels.write().insert(id, channel.clone());
		channel
	}

	/// Removes channel with given id.
	pub fn remove(&self, id: PeerId) -> Option<Arc<Channel>> {
		self.channels.write().remove(&id)
	}

	/// Drop all channels.
	pub fn remove_all(&self) -> Vec<Arc<Channel>> {
		mem::replace(&mut *self.channels.write(), HashMap::new())
			.into_iter()
			.map(|(_, value)| value)
			.collect()
	}
}
