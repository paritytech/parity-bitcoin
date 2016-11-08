use std::sync::Arc;
use std::collections::VecDeque;
use parking_lot::Mutex;
use bytes::Bytes;
use message::{Command, Error};
use p2p::Context;
use net::Channel;
use protocol::{Protocol, PingProtocol, SyncProtocol, AddrProtocol, SeednodeProtocol};
use PeerId;

pub trait SessionFactory {
	fn new_session(context: Arc<Context>, peer: PeerId) -> Session;
}

pub struct SeednodeSessionFactory;

impl SessionFactory for SeednodeSessionFactory {
	fn new_session(context: Arc<Context>, peer: PeerId) -> Session {
		let ping = PingProtocol::new(context.clone(), peer).boxed();
		let addr = AddrProtocol::new(context.clone(), peer).boxed();
		let seed = SeednodeProtocol::new(context.clone(), peer).boxed();
		Session::new(vec![ping, addr, seed])
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

#[derive(Default)]
struct MessagesQueue {
	pub is_processing: bool,
	pub queue: VecDeque<(Command, Bytes)>,
}

pub struct Session {
	protocols: Mutex<Vec<Box<Protocol>>>,
	messages: Mutex<MessagesQueue>,
}

impl Session {
	pub fn new(protocols: Vec<Box<Protocol>>) -> Self {
		Session {
			protocols: Mutex::new(protocols),
			messages: Mutex::new(MessagesQueue::default()),
		}
	}

	pub fn initialize(&self, channel: Arc<Channel>) {
		for protocol in self.protocols.lock().iter_mut() {
			protocol.initialize(channel.peer_info().direction, channel.version());
		}
	}

	pub fn on_message(&self, channel: Arc<Channel>, command: Command, payload: Bytes) -> Result<(), Error> {
		{
			let mut messages = self.messages.lock();
			messages.queue.push_back((command, payload));
			if messages.is_processing {
				return Ok(())
			}

			messages.is_processing = true;
		}

		while let Some((command, payload)) = {
			let mut messages = self.messages.lock();
			match messages.queue.pop_front() {
				Some(message) => Some(message),
				None => {
					messages.is_processing = false;
					None
				}
			}
		} {
			if let Err(error) = self.process_message(&channel, command, payload) {
				return Err(error);
			}
		}

		Ok(())
	}

	pub fn on_close(&self) {
		for protocol in self.protocols.lock().iter_mut() {
			protocol.on_close();
		}
	}

	fn process_message(&self, channel: &Arc<Channel>, command: Command, payload: Bytes) -> Result<(), Error> {
		self.protocols.lock()
			.iter_mut()
			.map(|protocol| {
				protocol.on_message(&command, &payload, channel.version())
			})
			.collect::<Result<Vec<_>, Error>>()
			.map(|_| ())
	}
}

