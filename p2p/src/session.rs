use std::sync::Arc;
use parking_lot::Mutex;
use futures::collect;
use bytes::Bytes;
use message::{Command, Error};
use p2p::Context;
use net::Channel;
use protocol::{Protocol, PingProtocol, SyncProtocol, Direction};
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

	pub fn initialize(&self, channel: Arc<Channel>, direction: Direction) -> Result<(), Error> {
		self.protocols.lock()
			.iter_mut()
			.map(|protocol| {
				protocol.initialize(direction, channel.version())
			})
			.collect::<Result<Vec<_>, Error>>()
			.map(|_| ())
	}

	pub fn on_message(&self, channel: Arc<Channel>, command: Command, payload: Bytes) -> Result<(), Error> {
		self.protocols.lock()
			.iter()
			.map(|protocol| {
				protocol.on_message(&command, &payload, channel.version())
			})
			.collect::<Result<Vec<_>, Error>>()
			.map(|_| ())
	}
}

