use bytes::Bytes;
use message::{Error, Payload, deserialize_payload, serialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::{Protocol, ProtocolAction, Direction};
use util::nonce::{NonceGenerator, RandomNonce};

pub struct PingProtocol<T = RandomNonce> {
	/// Nonce generator
	nonce_generator: T,
	/// Last nonce sent in a ping message.
	last_ping_nonce: u64,
}

impl PingProtocol {
	pub fn new() -> Self {
		PingProtocol {
			nonce_generator: RandomNonce::default(),
			last_ping_nonce: 0,
		}
	}
}

impl<T> Protocol for PingProtocol<T> where T: NonceGenerator + Send {
	fn initialize(&mut self, direction: Direction, version: u32) -> Result<ProtocolAction, Error> {
		match direction {
			Direction::Outbound => Ok(ProtocolAction::None),
			Direction::Inbound => {
				let nonce = self.nonce_generator.get();
				self.last_ping_nonce = nonce;
				let ping = Ping::new(nonce);
				let serialized = try!(serialize_payload(&ping, version));
				Ok(ProtocolAction::Reply((Ping::command().into(), serialized)))
			},
		}
	}

	fn on_message(&self, command: &Command, payload: &Bytes, version: u32) -> Result<ProtocolAction, Error> {
		if command == &Ping::command().into() {
			let ping: Ping = try!(deserialize_payload(payload, version));
			let pong = Pong::new(ping.nonce);
			let serialized = try!(serialize_payload(&pong, version));
			Ok(ProtocolAction::Reply((Pong::command().into(), serialized)))
		} else if command == &Pong::command().into() {
			let pong: Pong = try!(deserialize_payload(payload, version));
			if pong.nonce != self.last_ping_nonce {
				Err(Error::InvalidCommand)
			} else {
				Ok(ProtocolAction::None)
			}
		} else {
			Ok(ProtocolAction::None)
		}
	}
}

