use std::{ops, net};
use futures::Poll;
use futures::stream::Stream;
use tokio_core::{net as tnet};
use tokio_core::reactor::Handle;
use tcp::TcpStream;
use Error;

pub struct Incoming(tnet::Incoming);

impl Stream for Incoming {
	type Item = (TcpStream, net::SocketAddr);
	type Error = Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		match try_ready!(self.0.poll()) {
			Some((stream, addr)) => {
				let stream: TcpStream = stream.into();
				Ok(Some((stream, addr)).into())
			},
			None => Ok(None.into()),
		}
	}
}

pub struct TcpListener(tnet::TcpListener);

impl ops::Deref for TcpListener {
	type Target = tnet::TcpListener;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl TcpListener {
	pub fn bind(addr: &net::SocketAddr, handle: &Handle) -> Result<TcpListener, Error> {
		let listener = try!(tnet::TcpListener::bind(addr, handle));
		Ok(TcpListener(listener))
	}

	pub fn incoming(self) -> Incoming {
		Incoming(self.0.incoming())
	}
}

