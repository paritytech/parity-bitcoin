use std::{ops, net, io};
use futures::{Future, Poll, Async};
use futures::stream::{Stream, BoxStream};
use tokio_core::{net as tnet};
use tokio_core::reactor::Handle;
use io::Error;

pub struct TcpStreamNew(tnet::TcpStreamNew);

impl Future for TcpStreamNew {
	type Item = TcpStream;
	type Error = Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let stream = try_ready!(self.0.poll());
		Ok(Async::Ready(TcpStream(stream)))
	}
}

pub struct TcpStream(tnet::TcpStream);

impl From<tnet::TcpStream> for TcpStream {
	fn from(s: tnet::TcpStream) -> Self {
		TcpStream(s)
	}
}

impl ops::Deref for TcpStream {
	type Target = tnet::TcpStream;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl io::Read for TcpStream {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		self.0.read(buf)
	}
}

impl io::Write for TcpStream {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
		self.0.write(buf)
	}

	fn flush(&mut self) -> Result<(), io::Error> {
		self.0.flush()
	}
}

impl TcpStream {
	pub fn connect(addr: &net::SocketAddr, handle: &Handle) -> TcpStreamNew {
		TcpStreamNew(tnet::TcpStream::connect(addr, handle))
	}
}

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

pub type IoStream<T> = BoxStream<T, Error>;
