use std::{io, net};
use std::sync::Arc;
use futures::{Future, finished};
use futures::stream::Stream;
use futures_cpupool::CpuPool;
use tokio_core::reactor::Handle;
use net::{connect, listen, Connections, Subscriber};
use Config;

pub struct P2P {
	/// Global event loop handle.
	event_loop_handle: Handle,
	/// Worker thread pool.
	pool: CpuPool,
	/// P2P config.
	config: Config,
	/// Connections.
	connections: Arc<Connections>,
	/// Message subscriber.
	subscriber: Subscriber,
}

impl P2P {
	pub fn new(config: Config, handle: Handle) -> Self {
		let pool = CpuPool::new(4);

		P2P {
			event_loop_handle: handle.clone(),
			pool: pool.clone(),
			config: config,
			connections: Arc::new(Connections::new(pool, handle)),
			subscriber: Subscriber::default(),
		}
	}

	pub fn run(&self) -> Result<(), io::Error> {
		for seednode in self.config.seednodes.iter() {
			self.connect(*seednode)
		}

		self.listen()
	}

	fn connect(&self, ip: net::IpAddr) {
		let socket = net::SocketAddr::new(ip, self.config.connection.magic.port());
		let connections = self.connections.clone();
		let connection = connect(&socket, &self.event_loop_handle, &self.config.connection);
		let pool_work = self.pool.spawn(connection).then(move |x| {
			if let Ok(Ok(con)) = x {
				connections.store(con);
			}
			finished(())
		});
		self.event_loop_handle.spawn(pool_work);
	}

	fn listen(&self) -> Result<(), io::Error> {
		let listen = try!(listen(&self.event_loop_handle, self.config.connection.clone()));
		let connections = self.connections.clone();
		let server = listen.for_each(move |x| {
			if let Ok(con) = x {
				connections.store(con)
			}
			Ok(())
		}).then(|_| {
			finished(())
		});
		self.event_loop_handle.spawn(server);
		Ok(())
	}
}
