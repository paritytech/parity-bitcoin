use std::sync::Arc;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{Command, Error};
use p2p::Context;
use net::Channel;
use protocol::{Protocol, PingProtocol, SyncProtocol, AddrProtocol, Direction};
use PeerId;

pub trait SessionFactory {
	fn new_session(context: Arc<Context>, peer: PeerId) -> Session;
}

pub struct SeednodeSessionFactory;

impl SessionFactory for SeednodeSessionFactory {
	fn new_session(context: Arc<Context>, peer: PeerId) -> Session {
		let ping = PingProtocol::new(context.clone(), peer).boxed();
		let addr = AddrProtocol::new(context.clone(), peer).boxed();
		Session::new(vec![ping, addr])
	}
}

pub struct NormalSessionFactory;

impl SessionFactory for NormalSessionFactory {
	fn new_session(context: Arc<Context>, peer: PeerId) -> Session {
		let ping = PingProtocol::new(context.clone(), peer).boxed();
		let addr = AddrProtocol::new(context.clone(), peer).boxed();
		let sync = SyncProtocol::new(context, peer).boxed();
		Session::new(vec![ping, addr, sync])
	}
}

pub struct Session {
	protocols: Mutex<Vec<Box<Protocol>>>,
}

impl Session {
	pub fn new(protocols: Vec<Box<Protocol>>) -> Self {
		Session {
			protocols: Mutex::new(protocols),
		}
	}

	pub fn initialize(&self, channel: Arc<Channel>, direction: Direction) {
		for protocol in self.protocols.lock().iter_mut() {
			protocol.initialize(direction, channel.version());
		}
	}

	pub fn on_message(&self, channel: Arc<Channel>, command: Command, payload: Bytes) -> Result<(), Error> {
		self.protocols.lock()
			.iter_mut()
			.map(|protocol| {
				protocol.on_message(&command, &payload, channel.version())
			})
			.collect::<Result<Vec<_>, Error>>()
			.map(|_| ())
	}
}

