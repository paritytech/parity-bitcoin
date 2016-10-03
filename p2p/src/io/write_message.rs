use std::io;
use tokio_core::io::{WriteAll, write_all};
use bytes::Bytes;
use ser::{serialize};
use message::Message;

pub type WriteMessage<A> = WriteAll<A, Bytes>;

pub fn write_message<A>(a: A, message: &Message) -> WriteMessage<A> where A: io::Write {
	write_all(a, serialize(message))
}
