use std::{net, io};
use std::time::Duration;
use futures::{Future, Poll};
use tokio_core::reactor::Handle;
use tokio_core::net::TcpStream;
use message::{MessageResult, Magic};
use io::{accept_handshake, AcceptHandshake, Deadline, deadline};
use net::{Config, Connection};

pub fn accept_connection(stream: TcpStream, handle: &Handle, config: &Config, address: net::SocketAddr) -> Deadline<AcceptConnection> {
	let accept = AcceptConnection {
		handshake: accept_handshake(stream, config.magic, config.version(&address)),
		magic: config.magic,
		address: address,
	};

	deadline(Duration::new(5, 0), handle, accept).expect("Failed to create timeout")
}

pub struct AcceptConnection {
	handshake: AcceptHandshake<TcpStream>,
	magic: Magic,
	address: net::SocketAddr,
}

impl Future for AcceptConnection {
	type Item = MessageResult<Connection>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (stream, result) = try_ready!(self.handshake.poll());
		let result = match result {
			Ok(result) => result,
			Err(err) => return Ok(Err(err).into()),
		};
		let connection = Connection {
			stream: stream.into(),
			version: result.negotiated_version,
			magic: self.magic,
			services: result.version.services(),
			address: self.address,
		};
		Ok(Ok(connection).into())
	}
}
