use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Payload, deserialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::{Protocol, Direction};
use util::nonce::{NonceGenerator, RandomNonce};
use p2p::Context;
use PeerId;

pub trait PingContext: Send + Sync {
	fn send_to_peer<T>(context: Arc<Self>, peer: PeerId, payload: &T) where Self: Sized, T: Payload;
}

impl PingContext for Context {
	fn send_to_peer<T>(context: Arc<Self>, peer: PeerId, payload: &T) where T: Payload {
		let send = Context::send_to_peer(context.clone(), peer, payload);
		context.spawn(send);
	}
}

pub struct PingProtocol<T = RandomNonce, C = Context> {
	/// Context
	context: Arc<C>,
	/// Connected peer id.
	peer: PeerId,
	/// Nonce generator.
	nonce_generator: T,
	/// Last nonce sent in the ping message.
	last_ping_nonce: Option<u64>,
}

impl PingProtocol {
	pub fn new(context: Arc<Context>, peer: PeerId) -> Self {
		PingProtocol {
			context: context,
			peer: peer,
			nonce_generator: RandomNonce::default(),
			last_ping_nonce: None,
		}
	}
}

impl<T, C> Protocol for PingProtocol<T, C> where T: NonceGenerator + Send, C: PingContext {
	fn initialize(&mut self, _direction: Direction, _version: u32) {
		// bitcoind always sends ping, let's do the same
		let nonce = self.nonce_generator.get();
		self.last_ping_nonce = Some(nonce);
		let ping = Ping::new(nonce);
		PingContext::send_to_peer(self.context.clone(), self.peer, &ping);
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error> {
		if command == &Ping::command() {
			let ping: Ping = try!(deserialize_payload(payload, version));
			let pong = Pong::new(ping.nonce);
			PingContext::send_to_peer(self.context.clone(), self.peer, &pong);
		} else if command == &Pong::command() {
			let pong: Pong = try!(deserialize_payload(payload, version));
			if Some(pong.nonce) != self.last_ping_nonce.take() {
				return Err(Error::InvalidCommand)
			}
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use parking_lot::Mutex;
	use bytes::Bytes;
	use message::{Payload, serialize_payload};
	use message::types::{Ping, Pong};
	use util::nonce::StaticNonce;
	use protocol::{Protocol, Direction};
	use PeerId;
	use super::{PingProtocol, PingContext};

	#[derive(Default)]
	struct TestPingContext {
		version: u32,
		messages: Mutex<Vec<(PeerId, Bytes)>>,
	}

	impl PingContext for TestPingContext {
		fn send_to_peer<T>(context: Arc<Self>, peer: PeerId, payload: &T) where T: Payload {
			let value = (peer, serialize_payload(payload, context.version).unwrap());
			context.messages.lock().push(value);
		}
	}

	#[test]
	fn test_ping_init() {
		let ping_context = Arc::new(TestPingContext::default());
		let peer = 99;
		let nonce = 1000;
		let expected_message = serialize_payload(&Ping::new(nonce), 0).unwrap();
		let mut ping_protocol = PingProtocol {
			context: ping_context.clone(),
			peer: peer,
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: None,
		};

		ping_protocol.initialize(Direction::Inbound, 0);
		let messages: Vec<(PeerId, Bytes)> = ping_context.messages.lock().clone();
		assert_eq!(messages.len(), 1);
		assert_eq!(messages[0].0, peer);
		assert_eq!(messages[0].1, expected_message);
		assert_eq!(ping_protocol.last_ping_nonce, Some(nonce));
	}

	#[test]
	fn test_ping_on_message_ping() {
		let ping_context = Arc::new(TestPingContext::default());
		let peer = 99;
		let nonce = 1000;
		let command = "ping".into();
		let message = serialize_payload(&Ping::new(nonce), 0).unwrap();
		let expected_message = serialize_payload(&Pong::new(nonce), 0).unwrap();
		let mut ping_protocol = PingProtocol {
			context: ping_context.clone(),
			peer: peer,
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: None,
		};

		assert!(ping_protocol.on_message(&command, &message, 0).is_ok());
		let messages: Vec<(PeerId, Bytes)> = ping_context.messages.lock().clone();
		assert_eq!(messages.len(), 1);
		assert_eq!(messages[0].0, peer);
		assert_eq!(messages[0].1, expected_message);
		assert_eq!(ping_protocol.last_ping_nonce, None);
	}

	#[test]
	fn test_ping_on_message_pong() {
		let ping_context = Arc::new(TestPingContext::default());
		let peer = 99;
		let nonce = 1000;
		let command = "pong".into();
		let message = serialize_payload(&Pong::new(nonce), 0).unwrap();
		let mut ping_protocol = PingProtocol {
			context: ping_context.clone(),
			peer: peer,
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: Some(nonce),
		};

		assert!(ping_protocol.on_message(&command, &message, 0).is_ok());
		let messages: Vec<(PeerId, Bytes)> = ping_context.messages.lock().clone();
		assert_eq!(messages.len(), 0);
		assert_eq!(ping_protocol.last_ping_nonce, None);
	}
}
