use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Payload, deserialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::Protocol;
use util::{PeerId, PeerInfo};
use util::nonce::{NonceGenerator, RandomNonce};
use p2p::Context;

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
	/// Connected peer info.
	info: PeerInfo,
	/// Nonce generator.
	nonce_generator: T,
	/// Last nonce sent in the ping message.
	last_ping_nonce: Option<u64>,
}

impl PingProtocol {
	pub fn new(context: Arc<Context>, info: PeerInfo) -> Self {
		PingProtocol {
			context: context,
			info: info,
			nonce_generator: RandomNonce::default(),
			last_ping_nonce: None,
		}
	}
}

impl<T, C> Protocol for PingProtocol<T, C> where T: NonceGenerator + Send, C: PingContext {
	fn initialize(&mut self) {
		// bitcoind always sends ping, let's do the same
		let nonce = self.nonce_generator.get();
		self.last_ping_nonce = Some(nonce);
		let ping = Ping::new(nonce);
		PingContext::send_to_peer(self.context.clone(), self.info.id, &ping);
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		if command == &Ping::command() {
			let ping: Ping = try!(deserialize_payload(payload, self.info.version));
			let pong = Pong::new(ping.nonce);
			PingContext::send_to_peer(self.context.clone(), self.info.id, &pong);
		} else if command == &Pong::command() {
			let pong: Pong = try!(deserialize_payload(payload, self.info.version));
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
	use message::{Payload, serialize_payload, Magic};
	use message::types::{Ping, Pong};
	use util::{PeerId, PeerInfo, Direction};
	use util::nonce::StaticNonce;
	use protocol::Protocol;
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
			info: PeerInfo {
				id: peer,
				address: "0.0.0.0:8080".parse().unwrap(),
				direction: Direction::Inbound,
				version: 0,
				magic: Magic::Testnet,
			},
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: None,
		};

		ping_protocol.initialize();
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
			info: PeerInfo {
				id: peer,
				address: "0.0.0.0:8080".parse().unwrap(),
				direction: Direction::Inbound,
				version: 0,
				magic: Magic::Testnet,
			},
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: None,
		};

		assert!(ping_protocol.on_message(&command, &message).is_ok());
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
			info: PeerInfo {
				id: peer,
				address: "0.0.0.0:8080".parse().unwrap(),
				direction: Direction::Inbound,
				version: 0,
				magic: Magic::Testnet,
			},
			nonce_generator: StaticNonce::new(nonce),
			last_ping_nonce: Some(nonce),
		};

		assert!(ping_protocol.on_message(&command, &message).is_ok());
		let messages: Vec<(PeerId, Bytes)> = ping_context.messages.lock().clone();
		assert_eq!(messages.len(), 0);
		assert_eq!(ping_protocol.last_ping_nonce, None);
	}
}
