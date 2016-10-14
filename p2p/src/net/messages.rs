use std::io;
use std::sync::Weak;
use bytes::Bytes;
use futures::{Poll, Async};
use futures::stream::Stream;
use message::common::Command;
use net::Connections;
use PeerId;

pub struct MessagesHandler {
	last_polled: usize,
	connections: Weak<Connections>,
}

fn next_to_poll(channels: usize, last_polled: usize) -> usize {
	// it's irrelevant if we sometimes poll the same peer
	if channels > last_polled + 1 {
		// let's poll the next peer
		last_polled + 1
	} else {
		// let's move to the first channel
		0
	}
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
	type Item = (Command, Bytes, u32, PeerId);
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

		let mut to_poll = next_to_poll(channels.len(), self.last_polled);
		let mut result = None;

		while result.is_none() && to_poll != self.last_polled {
			let (id, channel) = channels.iter().nth(to_poll).expect("to_poll < channels.len()");
			let status = channel.poll_message();

			match status {
				Ok(Async::Ready(Some(Ok((command, message))))) => {
					result = Some((command, message, channel.version(), *id));
				},
				Ok(Async::NotReady) => {
					// no messages yet, try next channel
					to_poll = next_to_poll(channels.len(), to_poll);
				},
				_ => {
					// channel has been closed or there was error
					connections.remove(*id);
					to_poll = next_to_poll(channels.len(), to_poll);
				},
			}
		}

		self.last_polled = to_poll;
		match result.is_some() {
			true => Ok(Async::Ready(result)),
			false => Ok(Async::NotReady),
		}
	}
}


