use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Payload, deserialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::Protocol;
use net::PeerContext;
use util::nonce::{NonceGenerator, RandomNonce};

pub struct PingProtocol<T = RandomNonce, C = PeerContext> {
	/// Context
	context: Arc<C>,
	/// Nonce generator.
	nonce_generator: T,
	/// Last nonce sent in the ping message.
	last_ping_nonce: Option<u64>,
}

impl PingProtocol {
	pub fn new(context: Arc<PeerContext>) -> Self {
		PingProtocol {
			context: context,
			nonce_generator: RandomNonce::default(),
			last_ping_nonce: None,
		}
	}
}

impl Protocol for PingProtocol {
	fn initialize(&mut self) {
		// bitcoind always sends ping, let's do the same
		let nonce = self.nonce_generator.get();
		self.last_ping_nonce = Some(nonce);
		let ping = Ping::new(nonce);
		self.context.send_request(&ping);
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		if command == &Ping::command() {
			let ping: Ping = try!(deserialize_payload(payload, self.context.info().version));
			let pong = Pong::new(ping.nonce);
			self.context.send_response_inline(&pong);
		} else if command == &Pong::command() {
			let pong: Pong = try!(deserialize_payload(payload, self.context.info().version));
			if Some(pong.nonce) != self.last_ping_nonce.take() {
				return Err(Error::InvalidCommand)
			}
		}

		Ok(())
	}
}
