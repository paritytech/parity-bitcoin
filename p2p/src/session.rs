use std::sync::Arc;
use parking_lot::Mutex;
use tokio_core::io::IoFuture;
use bytes::Bytes;
use message::Command;
use p2p::Context;
use net::Channel;
use protocol::{Protocol, PingProtocol};

pub struct Session {
	protocols: Vec<Arc<Mutex<Box<Protocol>>>>,
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
			protocols: protocols.into_iter().map(Mutex::new).map(Arc::new).collect(),
		}
	}

	pub fn initialize(&self, _context: Arc<Context>, _channel: Arc<Channel>) -> IoFuture<()> {
		unimplemented!();
	}

	pub fn on_message(&self, _context: Arc<Context>, _channel: Arc<Channel>, _command: Command, _payload: Bytes) -> IoFuture<()> {
		unimplemented!();
	}
}

