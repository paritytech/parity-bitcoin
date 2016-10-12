use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::collections::HashMap;
use parking_lot::RwLock;
use futures::{finished, Oneshot, Future};
use futures_cpupool::CpuPool;
use tokio_core::reactor::Handle;
use message::PayloadType;
use net::Connection;
use PeerId;

pub struct Connections {
	event_loop_handle: Handle,
	pool: CpuPool,
	peer_counter: AtomicUsize,
	channels: RwLock<HashMap<PeerId, Arc<Connection>>>,
}

impl Connections {
	pub fn new(pool: CpuPool, handle: Handle) -> Self {
		Connections {
			event_loop_handle: handle,
			pool: pool,
			peer_counter: AtomicUsize::default(),
			channels: RwLock::default(),
		}
	}

	/// Broadcast messages to the network.
	/// Returned future completes of first confirmed receive.
	pub fn broadcast<T>(connections: &Arc<Connections>, payload: T) where T: PayloadType {
		let channels = connections.channels();
		for (id, channel) in channels.into_iter() {
			let write = channel.write_message(&payload);
			let cs = connections.clone();
			let pool_work = connections.pool.spawn(write).then(move |x| {
				match x {
					Ok(_) => {
						// successfully sent message
					},
					Err(_) => {
						cs.remove(id);
					}
				}
				finished(())
			});
			connections.event_loop_handle.spawn(pool_work);
		}
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
	pub fn store(&self, connection: Connection) {
		let id = self.peer_counter.fetch_add(1, Ordering::AcqRel);
		self.channels.write().insert(id, Arc::new(connection));
	}

	/// Removes channel with given id.
	pub fn remove(&self, id: PeerId) {
		self.channels.write().remove(&id);
	}
}
