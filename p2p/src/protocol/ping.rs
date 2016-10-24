use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Payload, deserialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::{Protocol, Direction};
use util::nonce::{NonceGenerator, RandomNonce};
use p2p::Context;
use PeerId;

pub struct PingProtocol<T = RandomNonce> {
	/// Context
	context: Arc<Context>,
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

impl<T> Protocol for PingProtocol<T> where T: NonceGenerator + Send {
	fn initialize(&mut self, _direction: Direction, _version: u32) -> Result<(), Error> {
		// bitcoind always sends ping, let's do the same
		let nonce = self.nonce_generator.get();
		self.last_ping_nonce = Some(nonce);
		let ping = Ping::new(nonce);
		let send = Context::send_to_peer(self.context.clone(), self.peer, &ping);
		self.context.spawn(send);
		Ok(())
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error> {
		if command == &Ping::command().into() {
			let ping: Ping = try!(deserialize_payload(payload, version));
			let pong = Pong::new(ping.nonce);
			let send = Context::send_to_peer(self.context.clone(), self.peer, &pong);
			self.context.spawn(send);
		} else if command == &Pong::command().into() {
			let pong: Pong = try!(deserialize_payload(payload, version));
			if Some(pong.nonce) != self.last_ping_nonce.take() {
				return Err(Error::InvalidCommand)
			}
		}

		Ok(())
	}
}

