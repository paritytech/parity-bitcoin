use std::io;
use std::sync::Weak;
use bytes::Bytes;
use futures::{Poll, Async};
use futures::stream::Stream;
use message::common::Command;
use net::Connections;
use PeerInfo;

pub enum MessagePoll {
	Ready {
		command: Command,
		payload: Bytes,
		version: u32,
		peer_info: PeerInfo,
		errored_peers: Vec<PeerInfo>,
	},
	OnlyErrors {
		errored_peers: Vec<PeerInfo>,
	}
}

pub struct MessagePoller {
	last_polled: usize,
	connections: Weak<Connections>,
}

fn next_to_poll(channels: usize, last_polled: usize) -> usize {
	// it's irrelevant if we sometimes poll the same peer twice in a row
	if channels > last_polled + 1 {
		// let's poll the next peer
		last_polled + 1
	} else {
		// let's move to the first channel
		0
	}
}

impl MessagePoller {
	pub fn new(connections: Weak<Connections>) -> Self {
		MessagePoller {
			last_polled: usize::max_value(),
			connections: connections,
		}
	}
}

impl Stream for MessagePoller {
	type Item = MessagePoll;
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
		let mut errored_peers = Vec::new();

		while result.is_none() && to_poll != self.last_polled {
			let (_, channel) = channels.iter().nth(to_poll).expect("to_poll < channels.len()");
			let status = channel.poll_message();

			match status {
				Ok(Async::Ready(Some(Ok((command, payload))))) => {
					result = Some((command, payload, channel.version(), channel.peer_info()));
				},
				Ok(Async::NotReady) => {
					// no messages yet, try next channel
					to_poll = next_to_poll(channels.len(), to_poll);
				},
				_ => {
					// channel has been closed or there was error
					errored_peers.push(channel.peer_info());
					to_poll = next_to_poll(channels.len(), to_poll);
				},
			}
		}

		self.last_polled = to_poll;
		match result {
			Some((command, payload, version, info)) => {
				let message_poll = MessagePoll::Ready {
					command: command,
					payload: payload,
					version: version,
					peer_info: info,
					errored_peers: errored_peers,
				};

				Ok(Async::Ready(Some(message_poll)))
			},
			None if errored_peers.is_empty() => Ok(Async::NotReady),
			_ => {
				let message_poll = MessagePoll::OnlyErrors {
					errored_peers: errored_peers,
				};
				Ok(Async::Ready(Some(message_poll)))
			}
		}
	}
}


