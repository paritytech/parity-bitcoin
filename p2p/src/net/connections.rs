use std::{mem, net};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::{HashMap, HashSet};
use parking_lot::RwLock;
use net::{Connection, Channel};
use p2p::Context;
use session::{SessionFactory};
use util::{Direction, PeerInfo};
use PeerId;

const SYNCHRONOUS_RESPONSES: bool = true;

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
	pub fn channels(&self) -> HashMap<PeerId, Arc<Channel>> {
		self.channels.read().clone()
	}

	/// Returns addresses of all active channels (nonblocking).
	pub fn addresses(&self) -> HashSet<net::SocketAddr> {
		self.channels().values().map(|channel| channel.peer_info().address).collect()
	}

	/// Returns info on every peer
	pub fn info(&self) -> Vec<PeerInfo> {
		self.channels().values().map(|channel| channel.peer_info()).collect()
	}

	/// Returns number of connections.
	pub fn count(&self) -> usize {
		self.channels.read().len()
	}

	/// Stores new channel.
	/// Returnes a shared pointer to it.
	pub fn store<T>(&self, context: Arc<Context>, connection: Connection, direction: Direction) -> Arc<Channel> where T: SessionFactory {
		let id = self.peer_counter.fetch_add(1, Ordering::AcqRel);

		let peer_info = PeerInfo {
			id: id,
			address: connection.address,
			user_agent: connection.version_message.user_agent().unwrap_or("unknown".into()),
			direction: direction,
			version: connection.version,
			version_message: connection.version_message,
			magic: connection.magic,
		};

		let session = T::new_session(context, peer_info.clone(), SYNCHRONOUS_RESPONSES);
		let channel = Arc::new(Channel::new(connection.stream, peer_info, session));
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
