use std::sync::Arc;
use time;
use bytes::Bytes;
use message::{Error, Payload, deserialize_payload};
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::Protocol;
use net::PeerContext;
use util::nonce::{NonceGenerator, RandomNonce};

/// Time that must pass since last message from this peer, before we send ping request
const PING_INTERVAL_S: f64 = 60f64;
/// If peer has not responded to our ping request with pong during this interval => close connection
const MAX_PING_RESPONSE_TIME_S: f64 = 60f64;

/// Ping state
#[derive(Debug, Copy, Clone, PartialEq)]
enum State {
	/// Peer is sending us messages && we wait for `PING_INTERVAL_S` to pass before sending ping request
	WaitingTimeout(f64),
	/// Ping message is sent to the peer && we are waiting for pong response for `MAX_PING_RESPONSE_TIME_S`
	WaitingPong(f64),
}

pub struct PingProtocol<T = RandomNonce, C = PeerContext> {
	/// Context
	context: Arc<C>,
	/// Nonce generator.
	nonce_generator: T,
	/// Ping state
	state: State,
	/// Last nonce sent in the ping message.
	last_ping_nonce: Option<u64>,
}

impl PingProtocol {
	pub fn new(context: Arc<PeerContext>) -> Self {
		PingProtocol {
			context: context,
			nonce_generator: RandomNonce::default(),
			state: State::WaitingTimeout(time::precise_time_s()),
			last_ping_nonce: None,
		}
	}
}

impl Protocol for PingProtocol {
	fn initialize(&mut self) {
		// bitcoind always sends ping, let's do the same
		self.maintain();
	}

	fn maintain(&mut self) {
		let now = time::precise_time_s();
		match self.state {
			State::WaitingTimeout(time) => {
				// send ping request if enough time has passed since last message
				if now - time > PING_INTERVAL_S {
					let nonce = self.nonce_generator.get();
					self.state = State::WaitingPong(now);
					self.last_ping_nonce = Some(nonce);
					let ping = Ping::new(nonce);
					self.context.send_request(&ping);
				}
			},
			State::WaitingPong(time) => {
				// if no new messages from peer for last MAX_PING_RESPONSE_TIME_S => disconnect
				if now - time > MAX_PING_RESPONSE_TIME_S {
					trace!("closing connection to peer {}: no messages for last {} seconds", self.context.info().id, now - time);
					self.context.close();
				}
			},
		}
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		// we have received new message => do not close connection because of timeout
		self.state = State::WaitingTimeout(time::precise_time_s());
		
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
