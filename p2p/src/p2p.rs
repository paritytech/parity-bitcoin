use std::{io, net, error, time};
use std::sync::Arc;
use std::net::SocketAddr;
use parking_lot::RwLock;
use futures::{Future, finished, failed};
use futures::stream::Stream;
use futures_cpupool::{CpuPool, Builder as CpuPoolBuilder};
use tokio_io::IoFuture;
use tokio_core::net::{TcpListener, TcpStream};
use tokio_core::reactor::{Handle, Remote, Timeout, Interval};
use abstract_ns::Resolver;
use ns_dns_tokio::DnsResolver;
use message::{Payload, MessageResult, Message};
use message::common::Services;
use message::types::addr::AddressEntry;
use net::{connect, Connections, Channel, Config as NetConfig, accept_connection, ConnectionCounter};
use util::{NodeTable, Node, NodeTableError, Direction};
use session::{SessionFactory, SeednodeSessionFactory, NormalSessionFactory};
use {Config, PeerId};
use protocol::{LocalSyncNodeRef, InboundSyncConnectionRef, OutboundSyncConnectionRef};
use io::DeadlineStatus;

pub type BoxedEmptyFuture = Box<dyn Future<Item=(), Error=()> + Send>;

/// Network context.
pub struct Context {
	/// Connections.
	connections: Connections,
	/// Connection counter.
	connection_counter: ConnectionCounter,
	/// Node Table.
	node_table: RwLock<NodeTable>,
	/// Thread pool handle.
	pool: CpuPool,
	/// Remote event loop handle.
	remote: Remote,
	/// Local synchronization node.
	local_sync_node: LocalSyncNodeRef,
	/// Node table path.
	config: Config,
}

impl Context {
	/// Creates new context with reference to local sync node, thread pool and event loop.
	pub fn new(local_sync_node: LocalSyncNodeRef, pool_handle: CpuPool, remote: Remote, config: Config) -> Result<Self, Box<dyn error::Error>> {
		let context = Context {
			connections: Default::default(),
			connection_counter: ConnectionCounter::new(config.inbound_connections, config.outbound_connections),
			node_table: RwLock::new(try!(NodeTable::from_file(config.preferable_services, &config.node_table_path))),
			pool: pool_handle,
			remote: remote,
			local_sync_node: local_sync_node,
			config: config,
		};

		Ok(context)
	}

	/// Spawns a future using thread pool and schedules execution of it with event loop handle.
	pub fn spawn<F>(&self, f: F) where F: Future + Send + 'static, F::Item: Send + 'static, F::Error: Send + 'static {
		let pool_work = self.pool.spawn(f);
		self.remote.spawn(move |_handle| {
			pool_work.then(|_| finished(()))
		})
	}

	/// Schedules execution of function in future.
	/// Use wisely, it keeps used objects in memory until after it is resolved.
	pub fn execute_after<F>(&self, duration: time::Duration, f: F) where F: FnOnce() + 'static + Send {
		let pool = self.pool.clone();
		self.remote.spawn(move |handle| {
			let timeout = Timeout::new(duration, handle)
				.expect("Expected to schedule timeout")
				.then(move |_| {
					f();
					finished(())
				});
			pool.spawn(timeout)
		});
	}

	/// Returns addresses of recently active nodes. Sorted and limited to 1000.
	pub fn node_table_entries(&self) -> Vec<Node> {
		self.node_table.read().recently_active_nodes(self.config.internet_protocol)
	}

	/// Updates node table.
	pub fn update_node_table(&self, nodes: Vec<AddressEntry>) {
		trace!("Updating node table with {} entries", nodes.len());
		self.node_table.write().insert_many(nodes);
	}

	/// Penalize node.
	pub fn penalize_node(&self, addr: &SocketAddr) {
		trace!("Penalizing node {}", addr);
		self.node_table.write().note_failure(addr);
	}

	/// Adds node to table.
	pub fn add_node(&self, addr: SocketAddr) -> Result<(), NodeTableError> {
		trace!("Adding node {} to node table", &addr);
		self.node_table.write().add(addr, self.config.connection.services)
	}

	/// Removes node from table.
	pub fn remove_node(&self, addr: SocketAddr) -> Result<(), NodeTableError> {
		trace!("Removing node {} from node table", &addr);
		self.node_table.write().remove(&addr)
	}

	/// Every 10 seconds check if we have reached maximum number of outbound connections.
	/// If not, connect to best peers.
	pub fn autoconnect(context: Arc<Context>, handle: &Handle) {
		let c = context.clone();
		// every 10 seconds connect to new peers (if needed)
		let interval: BoxedEmptyFuture = Box::new(Interval::new_at(time::Instant::now(), time::Duration::new(10, 0), handle).expect("Failed to create interval")
			.and_then(move |_| {
				// print traces
				let ic = context.connection_counter.inbound_connections();
				let oc = context.connection_counter.outbound_connections();
				info!("Inbound connections: ({}/{})", ic.0, ic.1);
				info!("Outbound connections: ({}/{})", oc.0, oc.1);

				for channel in context.connections.channels().values() {
					channel.session().maintain();
				}

				let needed = context.connection_counter.outbound_connections_needed() as usize;
				if needed != 0 {
					// TODO: pass Services::with_bitcoin_cash(true) after HF block
					let used_addresses = context.connections.addresses();
					let peers = context.node_table.read().nodes_with_services(&Services::default(), context.config.internet_protocol, &used_addresses, needed);
					let addresses = peers.into_iter()
						.map(|peer| peer.address())
						.collect::<Vec<_>>();

					trace!("Creating {} more outbound connections", addresses.len());
					for address in addresses {
						Context::connect::<NormalSessionFactory>(context.clone(), address);
					}
				}

				if let Err(_err) = context.node_table.read().save_to_file(&context.config.node_table_path) {
					error!("Saving node table to disk failed");
				}

				Ok(())
			})
			.for_each(|_| Ok(()))
			.then(|_| finished(())));
		c.spawn(interval);
	}

	/// Connect to socket using given context and handle.
	fn connect_future<T>(context: Arc<Context>, socket: net::SocketAddr, handle: &Handle, config: &NetConfig) -> BoxedEmptyFuture where T: SessionFactory {
		trace!("Trying to connect to: {}", socket);
		let connection = connect(&socket, handle, config);
		Box::new(connection.then(move |result| {
			match result {
				Ok(DeadlineStatus::Meet(Ok(connection))) => {
					// successfull hanshake
					trace!("Connected to {}", connection.address);
					context.node_table.write().insert(connection.address, connection.services);
					let channel = context.connections.store::<T>(context.clone(), connection, Direction::Outbound);

					// initialize session and then start reading messages
					channel.session().initialize();
					Context::on_message(context, channel)
				},
				Ok(DeadlineStatus::Meet(Err(_))) => {
					// protocol error
					trace!("Handshake with {} failed", socket);
					// TODO: close socket
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_outbound_connection();
					Box::new(finished(Ok(())))
				},
				Ok(DeadlineStatus::Timeout) => {
					// connection time out
					trace!("Handshake with {} timed out", socket);
					// TODO: close socket
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_outbound_connection();
					Box::new(finished(Ok(())))
				},
				Err(_) => {
					// network error
					trace!("Unable to connect to {}", socket);
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_outbound_connection();
					Box::new(finished(Ok(())))
				}
			}
		})
		.then(|_| finished(())))
	}

	/// Connect to socket using given context.
	pub fn connect<T>(context: Arc<Context>, socket: net::SocketAddr) where T: SessionFactory {
		context.connection_counter.note_new_outbound_connection();
		context.remote.clone().spawn(move |handle| {
			let config = context.config.clone();
			context.pool.clone().spawn(Context::connect_future::<T>(context, socket, handle, &config.connection))
		})
	}

	pub fn connect_normal(context: Arc<Context>, socket: net::SocketAddr) {
		Self::connect::<NormalSessionFactory>(context, socket)
	}

	pub fn accept_connection_future(context: Arc<Context>, stream: TcpStream, socket: net::SocketAddr, handle: &Handle, config: NetConfig) -> BoxedEmptyFuture {
		Box::new(accept_connection(stream, handle, &config, socket).then(move |result| {
			match result {
				Ok(DeadlineStatus::Meet(Ok(connection))) => {
					// successfull hanshake
					trace!("Accepted connection from {}", connection.address);
					context.node_table.write().insert(connection.address, connection.services);
					let channel = context.connections.store::<NormalSessionFactory>(context.clone(), connection, Direction::Inbound);

					// initialize session and then start reading messages
					channel.session().initialize();
					Context::on_message(context.clone(), channel)
				},
				Ok(DeadlineStatus::Meet(Err(err))) => {
					// protocol error
					trace!("Accepting handshake from {} failed with error: {}", socket, err);
					// TODO: close socket
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_inbound_connection();
					Box::new(finished(Ok(())))
				},
				Ok(DeadlineStatus::Timeout) => {
					// connection time out
					trace!("Accepting handshake from {} timed out", socket);
					// TODO: close socket
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_inbound_connection();
					Box::new(finished(Ok(())))
				},
				Err(_) => {
					// network error
					trace!("Accepting handshake from {} failed with network error", socket);
					context.node_table.write().note_failure(&socket);
					context.connection_counter.note_close_inbound_connection();
					Box::new(finished(Ok(())))
				}
			}
		})
		.then(|_| finished(())))
	}

	pub fn accept_connection(context: Arc<Context>, stream: TcpStream, socket: net::SocketAddr, config: NetConfig) {
		context.connection_counter.note_new_inbound_connection();
		context.remote.clone().spawn(move |handle| {
			context.pool.clone().spawn(Context::accept_connection_future(context, stream, socket, handle, config))
		})
	}

	/// Starts tcp server and listens for incomming connections.
	pub fn listen(context: Arc<Context>, handle: &Handle, config: NetConfig) -> Result<BoxedEmptyFuture, io::Error> {
		trace!("Starting tcp server");
		let server = try!(TcpListener::bind(&config.local_address, handle));
		let server = Box::new(server.incoming()
			.and_then(move |(stream, socket)| {
				// because we acquire atomic value twice,
				// it may happen that accept slightly more connections than we need
				// we don't mind
				if context.connection_counter.inbound_connections_needed() > 0 {
					Context::accept_connection(context.clone(), stream, socket, config.clone());
				} else {
					// ignore result
					let _ = stream.shutdown(net::Shutdown::Both);
				}
				Ok(())
			})
			.for_each(|_| Ok(()))
			.then(|_| finished(())));
		Ok(server)
	}

	/// Called on incomming mesage.
	pub fn on_message(context: Arc<Context>, channel: Arc<Channel>) -> IoFuture<MessageResult<()>> {
		Box::new(channel.read_message().then(move |result| {
			match result {
				Ok(Ok((command, payload))) => {
					// successful read
					trace!("Received {} message from {}", command, channel.peer_info().address);
					// handle message and read the next one
					match channel.session().on_message(command, payload) {
						Ok(_) => {
							context.node_table.write().note_used(&channel.peer_info().address);
							let on_message = Context::on_message(context.clone(), channel);
							context.spawn(on_message);
							Box::new(finished(Ok(())))
						},
						Err(err) => {
							// protocol error
							context.close_channel_with_error(channel.peer_info().id, &err);
							Box::new(finished(Err(err)))
						}
					}
				},
				Ok(Err(err)) => {
					// protocol error
					context.close_channel_with_error(channel.peer_info().id, &err);
					Box::new(finished(Err(err)))
				},
				Err(err) => {
					// network error
					// TODO: remote node was just turned off. should we mark it as not reliable?
					context.close_channel_with_error(channel.peer_info().id, &err);
					Box::new(failed(err))
				}
			}
		}))
	}

	/// Send message to a channel with given peer id.
	pub fn send_to_peer<T>(context: Arc<Context>, peer: PeerId, payload: &T, serialization_flags: u32) -> IoFuture<()> where T: Payload {
		match context.connections.channel(peer) {
			Some(channel) => {
				let info = channel.peer_info();
				let message = Message::with_flags(info.magic, info.version, payload, serialization_flags).expect("failed to create outgoing message");
				channel.session().stats().lock().report_send(T::command().into(), message.len());
				Context::send(context, channel, message)
			},
			None => {
				// peer no longer exists.
				// TODO: should we return error here?
				Box::new(finished(()))
			}
		}
	}

	pub fn send_message_to_peer<T>(context: Arc<Context>, peer: PeerId, message: T) -> IoFuture<()> where T: AsRef<[u8]> + Send + 'static {
		match context.connections.channel(peer) {
			Some(channel) => Context::send(context, channel, message),
			None => {
				// peer no longer exists.
				// TODO: should we return error here?
				Box::new(finished(()))
			}
		}
	}

	/// Send message using given channel.
	pub fn send<T>(_context: Arc<Context>, channel: Arc<Channel>, message: T) -> IoFuture<()> where T: AsRef<[u8]> + Send + 'static {
		//trace!("Sending {} message to {}", T::command(), channel.peer_info().address);
		Box::new(channel.write_message(message).then(move |result| {
			match result {
				Ok(_) => {
					// successful send
					//trace!("Sent {} message to {}", T::command(), channel.peer_info().address);
					Box::new(finished(()))
				},
				Err(err) => {
					// network error
					// closing connection is handled in on_message`
					Box::new(failed(err))
				},
			}
		}))
	}

	/// Close channel with given peer info.
	pub fn close_channel(&self, id: PeerId) {
		if let Some(channel) = self.connections.remove(id) {
			let info = channel.peer_info();
			channel.session().on_close();
			trace!("Disconnecting from {}", info.address);
			channel.shutdown();
			match info.direction {
				Direction::Inbound => self.connection_counter.note_close_inbound_connection(),
				Direction::Outbound => self.connection_counter.note_close_outbound_connection(),
			}
		}
	}

	/// Close channel with given peer info.
	pub fn close_channel_with_error(&self, id: PeerId, error: &dyn error::Error) {
		if let Some(channel) = self.connections.remove(id) {
			let info = channel.peer_info();
			channel.session().on_close();
			trace!("Disconnecting from {} caused by {}", info.address, error.description());
			channel.shutdown();
			self.node_table.write().note_failure(&info.address);
			match info.direction {
				Direction::Inbound => self.connection_counter.note_close_inbound_connection(),
				Direction::Outbound => self.connection_counter.note_close_outbound_connection(),
			}
		}
	}

	pub fn create_sync_session(&self, start_height: i32, services: Services, outbound_connection: OutboundSyncConnectionRef) -> InboundSyncConnectionRef {
		self.local_sync_node.create_sync_session(start_height, services, outbound_connection)
	}

	pub fn connections(&self) -> &Connections {
		&self.connections
	}

	pub fn nodes(&self) -> Vec<Node> {
		self.node_table.read().nodes()
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
	pub fn new(config: Config, local_sync_node: LocalSyncNodeRef, handle: Handle) -> Result<Self, Box<dyn error::Error>> {
		let pool = CpuPoolBuilder::new()
			.name_prefix("I/O thread")
			.pool_size(config.threads)
			.create();

		let context = try!(Context::new(local_sync_node, pool.clone(), handle.remote().clone(), config.clone()));

		let p2p = P2P {
			event_loop_handle: handle.clone(),
			pool: pool,
			context: Arc::new(context),
			config: config,
		};

		Ok(p2p)
	}

	pub fn run(&self) -> Result<(), Box<dyn error::Error>> {
		for peer in &self.config.peers {
			self.connect::<NormalSessionFactory>(*peer);
		}

		let resolver = try!(DnsResolver::system_config(&self.event_loop_handle));
		for seed in &self.config.seeds {
			self.connect_to_seednode(&resolver, seed);
		}

		Context::autoconnect(self.context.clone(), &self.event_loop_handle);
		try!(self.listen());
		Ok(())
	}

	/// Attempts to connect to the specified node
	pub fn connect<T>(&self, addr: net::SocketAddr) where T: SessionFactory {
		Context::connect::<T>(self.context.clone(), addr);
	}

	pub fn connect_to_seednode(&self, resolver: &dyn Resolver, seednode: &str) {
		let owned_seednode = seednode.to_owned();
		let context = self.context.clone();
		let dns_lookup = resolver.resolve(seednode).then(move |result| {
			match result {
				Ok(address) => match address.pick_one() {
					Some(socket) => {
						trace!("Dns lookup of seednode {} finished. Connecting to {}", owned_seednode, socket);
						Context::connect::<SeednodeSessionFactory>(context, socket);
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

	fn listen(&self) -> Result<(), Box<dyn error::Error>> {
		let server = try!(Context::listen(self.context.clone(), &self.event_loop_handle, self.config.connection.clone()));
		self.event_loop_handle.spawn(server);
		Ok(())
	}

	pub fn context(&self) -> &Arc<Context> {
		&self.context
	}
}
