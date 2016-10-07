use std::io;
use futures::{Future, Poll};
use tokio_core::io::{WriteAll, write_all};
use bytes::Bytes;
use ser::serialize;
use message::Message;

pub fn write_message<A>(a: A, message: Message) -> WriteMessage<A> where A: io::Write {
	WriteMessage {
		future: write_all(a, serialize(&message)),
		message: Some(message),
	}
}

pub struct WriteMessage<A> {
	future: WriteAll<A, Bytes>,
	message: Option<Message>,
}

impl<A> Future for WriteMessage<A> where A: io::Write {
	type Item = (A, Message);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
		let (stream, _) = try_ready!(self.future.poll());
		let message = self.message.take().expect("write message must be initialized with message");
		Ok((stream, message).into())
	}
}
