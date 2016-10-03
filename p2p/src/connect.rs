use std::{net, io};
use futures::{Future, Poll, Async};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use message::common::{Magic, ServiceFlags, NetAddress};
use message::types::{Version, Simple, V106, V70001};
use stream::{TcpStream, TcpStreamNew, TcpListener};
use io::{handshake, Handshake, HandshakeResult, accept_handshake, AcceptHandshake, IoStream};
use util::time::{Time, RealTime};
use util::nonce::{NonceGenerator, RandomNonce};
use {VERSION, Error};

pub struct Connection<A> where A: io::Read + io::Write {
	pub stream: A,
	pub handshake_result: HandshakeResult,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

#[derive(Debug, Clone)]
pub struct Config {
	pub magic: Magic,
	pub local_address: net::SocketAddr,
	pub services: ServiceFlags,
	pub user_agent: String,
	pub start_height: i32,
	pub relay: bool,
}

fn version(config: &Config, address: &net::SocketAddr) -> Version {
	Version::V70001(Simple {
		version: VERSION,
		services: config.services,
		timestamp: RealTime.get().sec,
		receiver: NetAddress {
			services: config.services,
			address: address.ip().into(),
			port: address.port().into(),
		},
	}, V106 {
		from: NetAddress {
			services: config.services,
			address: config.local_address.ip().into(),
			port: config.local_address.port().into(),
		},
		nonce: RandomNonce.get(),
		user_agent: config.user_agent.clone(),
		start_height: config.start_height,
	}, V70001 {
		relay: config.relay,
	})
}

pub fn connect(address: &net::SocketAddr, handle: &Handle, config: &Config) -> Connect {
	Connect {
		state: ConnectState::TcpConnect {
			future: TcpStream::connect(address, handle),
			version: Some(version(config, address)),
		},
		magic: config.magic,
		address: *address,
	}
}

pub fn listen(handle: &Handle, config: Config) -> Result<Listen, Error> {
	let listener = try!(TcpListener::bind(&config.local_address, handle));
	let listen = Listen {
		inner: listener.incoming()
			.and_then(move |(stream, address)| accept_connection(stream.into(), &config, address))
			.boxed(),
	};
	Ok(listen)
}

enum ConnectState {
	TcpConnect {
		future: TcpStreamNew,
		version: Option<Version>,
	},
	Handshake(Handshake<TcpStream>),
	Connected,
}

pub struct Connect {
	state: ConnectState,
	magic: Magic,
	address: net::SocketAddr,
}

pub struct Listen {
	inner: IoStream<Connection<TcpStream>>,
}

impl Future for Connect {
	type Item = Connection<TcpStream>;
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (next, result) = match self.state {
			ConnectState::TcpConnect { ref mut future, ref mut version } => {
				let stream = try_ready!(future.poll());
				let version = version.take().expect("state TcpConnect must have version");
				let handshake = handshake(stream, self.magic, version);
				(ConnectState::Handshake(handshake), Async::NotReady)
			},
			ConnectState::Handshake(ref mut future) => {
				let (stream, result) = try_ready!(future.poll());
				let connection = Connection {
					stream: stream,
					handshake_result: result,
					magic: self.magic,
					address: self.address,
				};
				(ConnectState::Connected, Async::Ready(connection))
			},
			ConnectState::Connected => panic!("poll Connect after it's done"),
		};

		self.state = next;
		match result {
			// by polling again, we register new future
			Async::NotReady => self.poll(),
			result => Ok(result)
		}
	}
}

fn accept_connection<A>(stream: A, config: &Config, address: net::SocketAddr) -> AcceptConnection<A> where A: io::Read + io::Write {
	AcceptConnection {
		handshake: accept_handshake(stream, config.magic, version(config, &address)),
		magic: config.magic,
		address: address,
	}
}

pub struct AcceptConnection<A> where A: io::Read + io::Write {
	handshake: AcceptHandshake<A>,
	magic: Magic,
	address: net::SocketAddr,
}

impl<A> Future for AcceptConnection<A> where A: io::Read + io::Write {
	type Item = Connection<A>;
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (stream, handshake_result) = try_ready!(self.handshake.poll());
		let connection = Connection {
			stream: stream,
			handshake_result: handshake_result,
			magic: self.magic,
			address: self.address,
		};
		Ok(connection.into())
	}
}

impl Stream for Listen {
	type Item = Connection<TcpStream>;
	type Error = Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		self.inner.poll()
	}
}
