use std::sync::Arc;
use parking_lot::Mutex;
use futures::{collect, finished, failed, Future};
use tokio_core::io::IoFuture;
use bytes::Bytes;
use message::Command;
use p2p::Context;
use net::Channel;
use protocol::{Protocol, ProtocolAction, PingProtocol, Direction};

pub struct Session {
	protocols: Mutex<Vec<Box<Protocol>>>,
}

impl Session {
	pub fn new() -> Self {
		let ping = PingProtocol::new();
		Session::new_with_protocols(vec![Box::new(ping)])
	}

	pub fn new_seednode() -> Self {
		let ping = PingProtocol::new();
		Session::new_with_protocols(vec![Box::new(ping)])
	}

	pub fn new_with_protocols(protocols: Vec<Box<Protocol>>) -> Self {
		Session {
			protocols: Mutex::new(protocols),
		}
	}

	pub fn initialize(&self, context: Arc<Context>, channel: Arc<Channel>) -> IoFuture<()> {
		let futures = self.protocols.lock()
			.iter_mut()
			.map(|protocol| {
				// TODO: use real direction and version
				match protocol.initialize(Direction::Inbound, 0) {
					Ok(ProtocolAction::None) => {
						finished(()).boxed()
					},
					Ok(ProtocolAction::Disconnect) => {
						// no other protocols can use the channel after that
						context.close_connection(channel.peer_info());
						finished(()).boxed()
					},
					Ok(ProtocolAction::Reply(message)) => {
						unimplemented!();
					},
					Err(err) => {
						// protocol error
						unimplemented!();
					}
				}
			})
			.collect::<Vec<_>>();
		collect(futures)
			.and_then(|_| finished(()))
			.boxed()
	}

	pub fn on_message(&self, context: Arc<Context>, channel: Arc<Channel>, command: Command, payload: Bytes) -> IoFuture<()> {
		let futures = self.protocols.lock()
			.iter()
			.map(|protocol| {
				// TODO: use real version
				match protocol.on_message(&command, &payload, 0) {
					Ok(ProtocolAction::None) => {
						finished(()).boxed()
					},
					Ok(ProtocolAction::Disconnect) => {
						context.close_connection(channel.peer_info());
						finished(()).boxed()
					},
					Ok(ProtocolAction::Reply(message)) => {
						unimplemented!();
					},
					Err(err) => {
						// protocol error
						unimplemented!();
					},
				}
			})
		.collect::<Vec<_>>();
		collect(futures)
			.and_then(|_| finished(()))
			.boxed()
	}
}

