use std::{io, net, error};
use std::sync::Arc;
use parking_lot::RwLock;
use futures::{Future, finished, failed, BoxFuture};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::io::IoFuture;
use tokio_core::reactor::{Handle, Remote};
use abstract_ns::Resolver;
use ns_dns_tokio::DnsResolver;
use message::{Payload, MessageResult};
use protocol::Direction;
use net::{connect, listen, Connections, Channel, Config as NetConfig};
use util::{NodeTable, Node};
use session::{SessionFactory, SeednodeSessionFactory, NormalSessionFactory};
use {Config, PeerInfo, PeerId};
use protocol::{LocalSyncNodeRef, InboundSyncConnectionRef, OutboundSyncConnectionRef};

pub type BoxedEmptyFuture = BoxFuture<(), ()>;

/// Network context.
pub struct Context {
	/// Connections.
	connections: Connections,
	/// Node Table.
	node_table: RwLock<NodeTable>,
	/// Thread pool handle.
	pool: CpuPool,
	/// Remote event loop handle.
	remote: Remote,
	/// Local synchronization node.
	local_sync_node: LocalSyncNodeRef,
}

impl Context {
	pub fn new(local_sync_node: LocalSyncNodeRef, pool_handle: CpuPool, remote: Remote) -> Self {
		Context {
			connections: Default::default(),
			node_table: Default::default(),
			pool: pool_handle,
			remote: remote,
			local_sync_node: local_sync_node,
		}
	}

	pub fn spawn<F>(&self, f: F) where F: Future + Send + 'static, F::Item: Send + 'static, F::Error: Send + 'static {
		let pool_work = self.pool.spawn(f);
		self.remote.spawn(move |handle| {
			handle.spawn(pool_work.then(|_| finished(())));
			Ok(())
		})
	}

	pub fn node_table_entries(&self) -> Vec<Node> {
		self.node_table.read().recently_active_nodes()
	}

	pub fn update_node_table(&self, nodes: Vec<Node>) {
		trace!("Updating node table with {} entries", nodes.len());
		self.node_table.write().insert_many(nodes);
	}

	pub fn connect<T>(context: Arc<Context>, socket: net::SocketAddr, handle: &Handle, config: &NetConfig) -> BoxedEmptyFuture where T: SessionFactory {
		trace!("Trying to connect to: {}", socket);
		let connection = connect(&socket, handle, config);
		connection.then(move |result| {
			match result {
				Ok(Ok(connection)) => {
					// successfull hanshake
					trace!("Connected to {}", connection.address);
					context.node_table.write().insert(connection.address, connection.services);
					let channel = context.connections.store::<T>(context.clone(), connection);

					// initialize session and then start reading messages
					channel.session().initialize(channel.clone(), Direction::Outbound);
					Context::on_message(context, channel)
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
					let channel = context.connections.store::<NormalSessionFactory>(context.clone(), connection);

					// initialize session and then start reading messages
					channel.session().initialize(channel.clone(), Direction::Inbound);
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

	pub fn on_message(context: Arc<Context>, channel: Arc<Channel>) -> IoFuture<MessageResult<()>> {
		channel.read_message().then(move |result| {
			match result {
				Ok(Ok((command, payload))) => {
					// successful read
					trace!("Received {} message from {}", command, channel.peer_info().address);
					// handle message and read the next one
					match channel.session().on_message(channel.clone(), command, payload) {
						Ok(_) => {
							context.node_table.write().note_used(&channel.peer_info().address);
							let on_message = Context::on_message(context.clone(), channel);
							context.spawn(on_message);
							finished(Ok(())).boxed()
						},
						Err(err) => {
							// protocol error
							context.close_connection(channel.peer_info());
							finished(Err(err)).boxed()
						}
					}
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

	pub fn send_to_peer<T>(context: Arc<Context>, peer: PeerId, payload: &T) -> IoFuture<()> where T: Payload {
		match context.connections.channel(peer) {
			Some(channel) => Context::send(context, channel, payload),
			None => {
				// peer no longer exists.
				// TODO: should we return error here?
				finished(()).boxed()
			}
		}
	}

	pub fn send<T>(_context: Arc<Context>, channel: Arc<Channel>, payload: &T) -> IoFuture<()> where T: Payload {
		trace!("Sending {} message to {}", T::command(), channel.peer_info().address);
		channel.write_message(payload).then(move |result| {
			match result {
				Ok(_) => {
					// successful send
					trace!("Sent {} message to {}", T::command(), channel.peer_info().address);
					finished(()).boxed()
				},
				Err(err) => {
					// network error
					// closing connection is handled in on_message`
					failed(err).boxed()
				},
			}
		}).boxed()
	}

	pub fn close_connection(&self, peer_info: PeerInfo) {
		if let Some(channel) = self.connections.remove(peer_info.id) {
			trace!("Disconnecting from {}", peer_info.address);
			channel.shutdown();
			self.node_table.write().note_failure(&peer_info.address);
		}
	}

	pub fn create_sync_session(&self, start_height: i32, outbound_connection: OutboundSyncConnectionRef) -> InboundSyncConnectionRef {
		self.local_sync_node.lock().create_sync_session(start_height, outbound_connection)
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

impl Drop for P2P {
	fn drop(&mut self) {
		// there are retain cycles
		// context->connections->channel->session->protocol->context
		// context->connections->channel->on_message closure->context
		// first let's get rid of session retain cycle
		for channel in &self.context.connections.remove_all() {
			// done, now let's finish on_message
			channel.shutdown();
		}
	}
}

impl P2P {
	pub fn new(config: Config, local_sync_node: LocalSyncNodeRef, handle: Handle) -> Self {
		let pool = CpuPool::new(config.threads);

		P2P {
			event_loop_handle: handle.clone(),
			pool: pool.clone(),
			config: config,
			context: Arc::new(Context::new(local_sync_node, pool, handle.remote().clone())),
		}
	}

	pub fn run(&self) -> Result<(), Box<error::Error>> {
		for peer in self.config.peers.iter() {
			self.connect::<NormalSessionFactory>(*peer);
		}

		let resolver = try!(DnsResolver::system_config(&self.event_loop_handle));
		for seed in self.config.seeds.iter() {
			self.connect_to_seednode(&resolver, seed);
		}

		try!(self.listen());
		Ok(())
	}

	pub fn connect<T>(&self, ip: net::IpAddr) where T: SessionFactory {
		let socket = net::SocketAddr::new(ip, self.config.connection.magic.port());
		let connection = Context::connect::<T>(self.context.clone(), socket, &self.event_loop_handle, &self.config.connection);
		let pool_work = self.pool.spawn(connection);
		self.event_loop_handle.spawn(pool_work);
	}

	pub fn connect_to_seednode(&self, resolver: &Resolver, seednode: &str) {
		let owned_seednode = seednode.to_owned();
		let context = self.context.clone();
		let remote = self.event_loop_handle.remote().clone();
		let connection_config = self.config.connection.clone();
		let dns_lookup = resolver.resolve(seednode).then(move |result| {
			match result {
				Ok(address) => match address.pick_one() {
					Some(socket) => {
						trace!("Dns lookup of seednode {} finished. Connecting to {}", owned_seednode, socket);
						remote.spawn(move |handle| {
							let connection = Context::connect::<SeednodeSessionFactory>(context.clone(), socket, handle, &connection_config);
							context.spawn(connection);
							Ok(())
						});
					},
					None => {
						trace!("Dns lookup of seednode {} resolved with no results", owned_seednode);
					}
				},
				Err(_err) => {
					trace!("Dns lookup of seednode {} failed", owned_seednode);
				}
			}
			finished(())
		});
		let pool_work = self.pool.spawn(dns_lookup);
		self.event_loop_handle.spawn(pool_work);
	}


	fn listen(&self) -> Result<(), Box<error::Error>> {
		let server = try!(Context::listen(self.context.clone(), &self.event_loop_handle, self.config.connection.clone()));
		let pool_work = self.pool.spawn(server);
		self.event_loop_handle.spawn(pool_work);
		Ok(())
	}
}
