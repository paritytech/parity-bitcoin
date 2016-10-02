use std::{net, io};
use futures::{Future, Poll, Async};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use tokio_core::net::TcpListener;
use tokio_core::io::IoStream;
use net::common::{Magic, ServiceFlags, NetAddress};
use net::messages::{Version, Simple};
use stream::{TcpStream, TcpStreamNew};
use io::{handshake, Handshake, HandshakeResult, Error, accept_handshake, AcceptHandshake, VERSION};
use util::time::{Time, RealTime};

pub struct Connection<A> where A: io::Read + io::Write {
	pub stream: A,
	pub handshake_result: HandshakeResult,
	pub magic: Magic,
	pub address: net::SocketAddr,
}

#[derive(Debug, Clone)]
pub struct Config {
	magic: Magic,
	port: u16,
	services: ServiceFlags,
	user_agent: String,
	start_height: i32,
	relay: bool,
}

/// TODO: must be VERSION 70_001
fn version(config: &Config, address: &net::SocketAddr) -> Version {
	Version::Simple(Simple {
		version: VERSION,
		services: config.services,
		timestamp: RealTime.get().sec,
		receiver: NetAddress {
			services: config.services,
			address: address.ip().into(),
			port: address.port().into(),
		},
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

pub fn listen(address: &net::SocketAddr, handle: &Handle, config: Config) -> Result<Listen, Error> {
	let listener = try!(TcpListener::bind(address, handle));
	let listen = Listen {
		incoming: listener.incoming()
			.map(move |(stream, address)| accept_connection(stream.into(), &config, address))
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
	incoming: IoStream<AcceptConnection<TcpStream>>,
}

impl Listen {
	pub fn incoming(self) -> IoStream<AcceptConnection<TcpStream>> {
		self.incoming
	}
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
