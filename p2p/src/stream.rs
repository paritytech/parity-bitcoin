use std::{ops, net, io};
use futures::{Future, Poll, Async};
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
