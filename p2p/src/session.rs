use std::sync::Arc;
use parking_lot::Mutex;
use futures::{collect, finished, failed, Future};
use tokio_core::io::IoFuture;
use bytes::Bytes;
use message::Command;
use p2p::Context;
use net::Channel;
use protocol::{Protocol, ProtocolAction, PingProtocol, SyncProtocol, Direction};
use PeerId;

pub struct Session {
	protocols: Mutex<Vec<Box<Protocol>>>,
}

impl Session {
	pub fn new(context: Arc<Context>, peer: PeerId) -> Self {
		let ping = PingProtocol::new().boxed();
		let sync = SyncProtocol::new(context, peer).boxed();
		Session::new_with_protocols(vec![ping, sync])
	}

	pub fn new_seednode() -> Self {
		let ping = PingProtocol::new().boxed();
		Session::new_with_protocols(vec![ping])
	}

	pub fn new_with_protocols(protocols: Vec<Box<Protocol>>) -> Self {
		Session {
			protocols: Mutex::new(protocols),
		}
	}

	pub fn initialize(&self, context: Arc<Context>, channel: Arc<Channel>, direction: Direction) -> IoFuture<()> {
		let futures = self.protocols.lock()
			.iter_mut()
			.map(|protocol| {
				match protocol.initialize(direction, channel.version()) {
					Ok(ProtocolAction::None) => {
						finished(()).boxed()
					},
					Ok(ProtocolAction::Disconnect) => {
						// no other protocols can use the channel after that
						context.close_connection(channel.peer_info());
						finished(()).boxed()
					},
					Ok(ProtocolAction::Reply((command, payload))) => {
						Context::send_raw(context.clone(), channel.clone(), command, &payload)
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
				match protocol.on_message(&command, &payload, channel.version()) {
					Ok(ProtocolAction::None) => {
						finished(()).boxed()
					},
					Ok(ProtocolAction::Disconnect) => {
						context.close_connection(channel.peer_info());
						finished(()).boxed()
					},
					Ok(ProtocolAction::Reply((command, payload))) => {
						Context::send_raw(context.clone(), channel.clone(), command, &payload)
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

