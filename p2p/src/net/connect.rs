use std::net::SocketAddr;
use futures::{Future, Poll, Async};
use tokio_core::reactor::Handle;
use message::common::Magic;
use message::types::Version;
use io::{handshake, Handshake};
use tcp::{TcpStream, TcpStreamNew};
use net::{Config, Connection};
use Error;

pub fn connect(address: &SocketAddr, handle: &Handle, config: &Config) -> Connect {
	Connect {
		state: ConnectState::TcpConnect {
			future: TcpStream::connect(address, handle),
			version: Some(config.version(address)),
		},
		magic: config.magic,
		address: *address,
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
	address: SocketAddr,
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
