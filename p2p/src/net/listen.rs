use std::{net, io};
use futures::{Future, Poll};
use futures::stream::Stream;
use tokio_core::reactor::Handle;
use message::common::Magic;
use io::{accept_handshake, AcceptHandshake, IoStream};
use net::{Config, Connection};
use tcp::{TcpStream, TcpListener};
use Error;

pub fn listen(handle: &Handle, config: Config) -> Result<Listen, Error> {
	let listener = try!(TcpListener::bind(&config.local_address, handle));
	let listen = Listen {
		inner: listener.incoming()
			.and_then(move |(stream, address)| accept_connection(stream.into(), &config, address))
			.boxed(),
	};
	Ok(listen)
}


pub struct Listen {
	inner: IoStream<Connection<TcpStream>>,
}

fn accept_connection<A>(stream: A, config: &Config, address: net::SocketAddr) -> AcceptConnection<A> where A: io::Read + io::Write {
	AcceptConnection {
		handshake: accept_handshake(stream, config.magic, config.version(&address)),
		magic: config.magic,
		address: address,
	}
}

struct AcceptConnection<A> where A: io::Read + io::Write {
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
