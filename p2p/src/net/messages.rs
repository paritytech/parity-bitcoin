use std::io;
use std::sync::Weak;
use bytes::Bytes;
use futures::{Poll, Async};
use futures::stream::Stream;
use message::common::Command;
use net::Connections;
use PeerInfo;

/// Reads and concatenates incoming messages from all connections
/// into a single messages stream.
pub struct MessagePoller {
	last_polled: usize,
	connections: Weak<Connections>,
}

/// Result of polling connections for new message.
pub enum MessagePoll {
	/// Returned on new message.
	Ready {
		command: Command,
		payload: Bytes,
		version: u32,
		peer_info: PeerInfo,
		errored_peers: Vec<PeerInfo>,
	},
	/// Returned when there are no new messages
	/// and some peers disconnected.
	OnlyErrors {
		errored_peers: Vec<PeerInfo>,
	},
	/// Returned when there are not peers to poll.
	WaitingForPeers,
}

fn next_to_poll(channels: usize, last_polled: usize) -> usize {
	// it's irrelevant if we sometimes poll the same peer twice in a row
	if channels - 1 > last_polled {
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
			// let's end the stream
			None => return Ok(None.into())
		};

		let channels = connections.channels();
		match channels.len() {
			0 => {
			// stream is not ready for reading messages
			// returning Async::NotReady, would require scheduling a poll
			// returning Async::Ready with custom value is hash, cause next
			// poll will be automatically scheduled
				Ok(Async::Ready(Some(MessagePoll::WaitingForPeers)))
			},
			1 => {
				let (_, channel) = channels.iter().nth(0).expect("0 < channels.len(); qed");

				match channel.poll_message() {
					Ok(Async::Ready(Some(Ok((command, payload))))) => {
						let message_poll = MessagePoll::Ready {
							command: command,
							payload: payload,
							version: channel.version(),
							peer_info: channel.peer_info(),
							errored_peers: Vec::new(),
						};

						Ok(Async::Ready(Some(message_poll)))
					},
					Ok(Async::NotReady) => {
						Ok(Async::NotReady)
					},
					_ => {
						let message_poll = MessagePoll::OnlyErrors {
							errored_peers: vec![channel.peer_info()],
						};
						Ok(Async::Ready(Some(message_poll)))
					}
				}
			},
			n => {

				let mut to_poll = next_to_poll(n, self.last_polled);
				let mut result = None;
				let mut errored_peers = Vec::new();

				while result.is_none() && to_poll != self.last_polled {
					let (_, channel) = channels.iter().nth(to_poll).expect("to_poll < channels.len(); qed");

					match channel.poll_message() {
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
	}
}


