use std::io;
use futures::{Future, Poll};
use tokio_io::AsyncWrite;
use tokio_io::io::{WriteAll, write_all};
use message::Message;

pub fn write_message<M, A>(a: A, message: Message<M>) -> WriteMessage<M, A> where A: AsyncWrite {
	WriteMessage {
		future: write_all(a, message),
	}
}

pub struct WriteMessage<M, A> {
	future: WriteAll<A, Message<M>>,
}

impl<M, A> Future for WriteMessage<M, A> where A: AsyncWrite {
	type Item = (A, Message<M>);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		self.future.poll()
	}
}
