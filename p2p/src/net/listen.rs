use std::{net, io};
use futures::{Future, Poll};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use tokio_core::net::{TcpStream, TcpListener};
use tokio_core::io::IoStream;
use message::common::Magic;
use io::{accept_handshake, AcceptHandshake};
use net::{Config, Connection};
use Error;

pub fn listen(handle: &Handle, config: Config) -> Result<Listen, io::Error> {
	let listener = try!(TcpListener::bind(&config.local_address, handle));
	let listen = Listen {
		inner: listener.incoming()
			.and_then(move |(stream, address)| accept_connection(stream, &config, address))
			.boxed(),
	};
	Ok(listen)
}


pub struct Listen {
	inner: IoStream<Result<Connection, Error>>,
}

fn accept_connection(stream: TcpStream, config: &Config, address: net::SocketAddr) -> AcceptConnection {
	AcceptConnection {
		handshake: accept_handshake(stream, config.magic, config.version(&address)),
		magic: config.magic,
		address: address,
	}
}

struct AcceptConnection {
	handshake: AcceptHandshake<TcpStream>,
	magic: Magic,
	address: net::SocketAddr,
}

impl Future for AcceptConnection {
	type Item = Result<Connection, Error>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (stream, result) = try_ready!(self.handshake.poll());
		let result = match result {
			Ok(result) => result,
			Err(err) => return Ok(Err(err).into()),
		};
		let connection = Connection {
			stream: stream,
			version: result.negotiated_version,
			magic: self.magic,
			address: self.address,
		};
		Ok(Ok(connection).into())
	}
}

impl Stream for Listen {
	type Item = Result<Connection, Error>;
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		self.inner.poll()
	}
}
