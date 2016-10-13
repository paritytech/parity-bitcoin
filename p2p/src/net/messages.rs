use std::io;
use std::sync::Weak;
use bytes::Bytes;
use futures::{Poll, Async};
use futures::stream::Stream;
use message::MessageResult;
use message::common::Command;
use net::Connections;
use PeerId;

pub struct MessagesHandler {
	last_polled: usize,
	connections: Weak<Connections>,
}

impl MessagesHandler {
	pub fn new(connections: Weak<Connections>) -> Self {
		MessagesHandler {
			last_polled: usize::max_value(),
			connections: connections,
		}
	}
}

impl Stream for MessagesHandler {
	type Item = (MessageResult<(Command, Bytes)>, u32, PeerId);
	type Error = io::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		let connections = match self.connections.upgrade() {
			Some(c) => c,
			// application is about to shutdown
			None => return Ok(None.into())
		};
		let channels = connections.channels();
		if channels.len() == 0 {
			// let's wait for some connections
			return Ok(Async::NotReady);
		}

		// it's irrelevant if we sometimes poll the same peer
		let to_poll = if channels.len() > self.last_polled + 1 {
			// let's poll the next peer
			self.last_polled + 1
		} else {
			// let's move to the first channel
			0
		};

		let (id, channel) = channels.into_iter().nth(to_poll).expect("to_poll < channels.len()");
		unimplemented!();
	}
}


