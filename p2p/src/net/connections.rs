use std::sync::Arc;
use std::collections::HashMap;
use parking_lot::RwLock;
use futures::{oneshot, Oneshot, Future};
use message::PayloadType;
use net::Connection;
use PeerId;

#[derive(Default)]
pub struct Connections {
	peer_counter: PeerId,
	channels: RwLock<HashMap<PeerId, Arc<Connection>>>,
}

impl Connections {
	/// Broadcast messages to the network.
	/// Returned future completes of first confirmed receive.
	/// TODO: make is async
	pub fn broadcast<T>(&self, payload: T) -> Oneshot<()> where T: PayloadType {
		let (complete, os) = oneshot::<()>();
		let mut complete = Some(complete);

		for (id, channel) in &self.channels() {
			let _wait = channel.write_message(&payload).map(|_message| {
				if let Some(complete) = complete.take() {
					complete.complete(());
				}
			}).map_err(|_err| {
				self.remove(*id)
			}).wait();
		}

		os
	}

	/// Returns safe (nonblocking) copy of channels.
	pub fn channels(&self) -> HashMap<PeerId, Arc<Connection>> {
		self.channels.read().clone()
	}

	/// Returns number of connections.
	pub fn count(&self) -> usize {
		self.channels.read().len()
	}

	/// Stores new channel.
	pub fn store(&mut self, connection: Connection) {
		self.channels.write().insert(self.peer_counter, Arc::new(connection));
		self.peer_counter += 1;
	}

	/// Removes channel with given id.
	pub fn remove(&self, id: PeerId) {
		self.channels.write().remove(&id);
	}
}
