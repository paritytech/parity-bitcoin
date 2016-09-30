use std::{net, io};
use futures::{Future, Poll, Async};
use tokio_core::reactor::Handle;
use net::common::{Magic, ServiceFlags};
use net::messages::Version;
use stream::{TcpStream, TcpStreamNew};
use io::{handshake, Handshake, HandshakeResult, Error};
use util::nonce::{NonceGenerator, RandomNonce};
use util::time::{Time, RealTime};

pub struct Connection<A> where A: io::Read + io::Write {
	pub stream: A,
	pub handshake_result: HandshakeResult,
	pub magic: Magic,
}

pub struct Config {
	magic: Magic,
	services: ServiceFlags,
	user_agent: String,
	start_height: i32,
	relay: bool,
}

fn version(config: &Config) -> Version {
	unimplemented!();
}

pub fn connect(addr: &net::SocketAddr, handle: &Handle, config: &Config) -> Connect {
	Connect {
		state: ConnectState::TcpConnect {
			future: TcpStream::connect(addr, handle),
			version: Some(version(config)),
		},
		magic: config.magic,
	}
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
