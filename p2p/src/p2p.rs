use std::{io, net};
use std::sync::Arc;
use parking_lot::RwLock;
use futures::{Future, finished, failed, BoxFuture};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::reactor::Handle;
use session::Session;
use io::{ReadAnyMessage, SharedTcpStream};
use net::{connect, listen, Connections, Channel, Config as NetConfig};
use util::NodeTable;
use {Config, PeerInfo};

pub type BoxedMessageFuture = BoxFuture<<ReadAnyMessage<SharedTcpStream> as Future>::Item, <ReadAnyMessage<SharedTcpStream> as Future>::Error>;
pub type BoxedEmptyFuture = BoxFuture<(), ()>;

/// Network context.
#[derive(Default)]
pub struct Context {
	/// Connections.
	connections: Connections,
	/// Node Table.
	node_table: RwLock<NodeTable>,
}

impl Context {
	pub fn connect(context: Arc<Context>, socket: net::SocketAddr, handle: &Handle, config: &NetConfig) -> BoxedEmptyFuture {
		trace!("Trying to connect to: {}", socket);
		let connection = connect(&socket, handle, config);
		connection.then(move |result| {
			match result {
				Ok(Ok(connection)) => {
					// successfull hanshake
					trace!("Connected to {}", connection.address);
					context.node_table.write().insert(connection.address, connection.services);
					let session = Session::new();
					let channel = context.connections.store(connection, session);
					Context::on_message(context.clone(), channel)
				},
				Ok(Err(err)) => {
					// protocol error
					trace!("Handshake with {} failed", socket);
					// TODO: close socket
					finished(Err(err)).boxed()
				},
				Err(err) => {
					// network error
					trace!("Unable to connect to {}", socket);
					failed(err).boxed()
				}
			}
		})
		.then(|_| finished(()))
		.boxed()
	}

	pub fn listen(context: Arc<Context>, handle: &Handle, config: NetConfig) -> Result<BoxedEmptyFuture, io::Error> {
		trace!("Starting tcp server");
		let listen = try!(listen(&handle, config));
		let server = listen.then(move |result| {
			match result {
				Ok(Ok(connection)) => {
					// successfull hanshake
					trace!("Accepted connection from {}", connection.address);
					context.node_table.write().insert(connection.address, connection.services);
					let session = Session::new();
					let channel = context.connections.store(connection, session);
					// read messages
					Context::on_message(context.clone(), channel)
				},
				Ok(Err(err)) => {
					// protocol error
					// TODO: close socket
					finished(Err(err)).boxed()
				},
				Err(err) => {
					// network error
					failed(err).boxed()
				}
			}
		})
		.for_each(|_| Ok(()))
		.then(|_| finished(()))
		.boxed();
		Ok(server)
	}

	pub fn on_message(context: Arc<Context>, channel: Arc<Channel>) -> BoxedMessageFuture {
		channel.read_message().then(move |result| {
			match result {
				Ok(Ok((command, _bytes))) => {
					// successful read
					trace!("Received {} message from {}", command, channel.peer_info().address);
					// read next messsage
					Context::on_message(context, channel)
				},
				Ok(Err(err)) => {
					// protocol error
					context.close_connection(channel.peer_info());
					finished(Err(err)).boxed()
				},
				Err(err) => {
					// network error
					context.close_connection(channel.peer_info());
					failed(err).boxed()
				}
			}
		}).boxed()
	}

	pub fn send(_context: &Arc<Context>) {
	}

	fn close_connection(&self, peer_info: PeerInfo) {
		if let Some(channel) = self.connections.remove(peer_info.id) {
			trace!("Disconnecting from {}", peer_info.address);
			channel.shutdown();
			self.node_table.write().note_failure(&peer_info.address);
		}
	}
}

pub struct P2P {
	/// Global event loop handle.
	event_loop_handle: Handle,
	/// Worker thread pool.
	pool: CpuPool,
	/// P2P config.
	config: Config,
	/// Network context.
	context: Arc<Context>,
}

impl P2P {
	pub fn new(config: Config, handle: Handle) -> Self {
		let pool = CpuPool::new(config.threads);

		P2P {
			event_loop_handle: handle.clone(),
			pool: pool.clone(),
			config: config,
			context: Arc::default(),
		}
	}

	pub fn run(&self) -> Result<(), io::Error> {
		for peer in self.config.peers.iter() {
			self.connect(*peer)
		}

		try!(self.listen());
		Ok(())
	}

	pub fn connect(&self, ip: net::IpAddr) {
		let socket = net::SocketAddr::new(ip, self.config.connection.magic.port());
		let connection = Context::connect(self.context.clone(), socket, &self.event_loop_handle, &self.config.connection);
		let pool_work = self.pool.spawn(connection);
		self.event_loop_handle.spawn(pool_work);
	}

	fn listen(&self) -> Result<(), io::Error> {
		let server = try!(Context::listen(self.context.clone(), &self.event_loop_handle, self.config.connection.clone()));
		let pool_work = self.pool.spawn(server);
		self.event_loop_handle.spawn(pool_work);
		Ok(())
	}

	/*
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
	*/
}
