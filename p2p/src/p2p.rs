use std::{io, net};
use std::sync::Arc;
use parking_lot::RwLock;
use futures::{Future, finished};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::reactor::Handle;
use message::Payload;
use net::{connect, listen, Connections, MessagePoller, MessagePoll, Channel};
use util::NodeTable;
use {Config, PeerId};

pub struct P2P {
	/// Global event loop handle.
	event_loop_handle: Handle,
	/// Worker thread pool.
	pool: CpuPool,
	/// P2P config.
	config: Config,
	/// Connections.
	connections: Arc<Connections>,
	/// Node Table.
	node_table: Arc<RwLock<NodeTable>>,
}

impl P2P {
	pub fn new(config: Config, handle: Handle) -> Self {
		let pool = CpuPool::new(config.threads);

		P2P {
			event_loop_handle: handle.clone(),
			pool: pool.clone(),
			config: config,
			connections: Arc::new(Connections::new()),
			node_table: Arc::default(),
		}
	}

	pub fn run(&self) -> Result<(), io::Error> {
		for seednode in self.config.peers.iter() {
			self.connect(*seednode)
		}

		try!(self.listen());
		self.attach_protocols();
		Ok(())
	}

	pub fn connect(&self, ip: net::IpAddr) {
		let socket = net::SocketAddr::new(ip, self.config.connection.magic.port());
		let connections = self.connections.clone();
		let node_table  = self.node_table.clone();
		let connection = connect(&socket, &self.event_loop_handle, &self.config.connection);
		let pool_work = self.pool.spawn(connection).then(move |result| {
			if let Ok(Ok(con)) = result {
				node_table.write().insert(con.address, con.services);
				connections.store(con);
			} else {
				node_table.write().note_failure(&socket);
			}
			finished(())
		});
		self.event_loop_handle.spawn(pool_work);
	}

	fn listen(&self) -> Result<(), io::Error> {
		let listen = try!(listen(&self.event_loop_handle, self.config.connection.clone()));
		let connections = self.connections.clone();
		let node_table  = self.node_table.clone();
		let server = listen.for_each(move |result| {
			if let Ok(con) = result {
				node_table.write().insert(con.address, con.services);
				connections.store(con);
			}
			Ok(())
		}).then(|_| {
			finished(())
		});
		let pool_work = self.pool.spawn(server);
		self.event_loop_handle.spawn(pool_work);
		Ok(())
	}

	fn attach_protocols(&self) {
		// TODO: here all network protocols will be attached

		let poller = MessagePoller::new(Arc::downgrade(&self.connections));
		let connections = self.connections.clone();
		let node_table  = self.node_table.clone();
		let polling = poller.for_each(move |result| {
			match result {
				MessagePoll::Ready { errored_peers, .. } => {
					// TODO: handle new messasges here!

					let mut node_table = node_table.write();
					for peer in errored_peers.into_iter() {
						node_table.note_failure(&peer.address);
						connections.remove(peer.id);
					}
				},
				MessagePoll::OnlyErrors { errored_peers } => {
					let mut node_table = node_table.write();
					for peer in errored_peers.into_iter() {
						node_table.note_failure(&peer.address);
						connections.remove(peer.id);
					}
				}
			}
			Ok(())
		}).then(|_| {
			finished(())
		});
		let pool_work = self.pool.spawn(polling);
		self.event_loop_handle.spawn(pool_work);
	}

	pub fn broadcast<T>(&self, payload: T) where T: Payload {
		let channels = self.connections.channels();
		for (_id, channel) in channels.into_iter() {
			self.send_to_channel(&payload, &channel);
		}
	}

	pub fn send<T>(&self, payload: T, peer: PeerId) where T: Payload {
		let channels = self.connections.channels();
		if let Some(channel) = channels.get(&peer) {
			self.send_to_channel(&payload, channel);
		}
	}

	fn send_to_channel<T>(&self, payload: &T, channel: &Arc<Channel>) where T: Payload {
		let connections = self.connections.clone();
		let node_table  = self.node_table.clone();
		let peer_info = channel.peer_info();
		let write = channel.write_message(payload);
		let pool_work = self.pool.spawn(write).then(move |result| {
			match result {
				Ok(_) => {
					node_table.write().note_used(&peer_info.address);
				},
				Err(_err) => {
					node_table.write().note_failure(&peer_info.address);
					connections.remove(peer_info.id);
				}
			}
			finished(())
		});
		self.event_loop_handle.spawn(pool_work);
	}
}
