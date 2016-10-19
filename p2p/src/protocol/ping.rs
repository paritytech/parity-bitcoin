use bytes::Bytes;
use message::types::{Ping, Pong};
use message::common::Command;
use protocol::{Protocol, ProtocolResult};

enum PingState {
	ExpectingPing,
	ExpectingPong,
}

pub struct PingProtocol {
	state: PingState,
}

impl Protocol for PingProtocol {
	fn initialize(&self) {
		// send ping to the peer
	}

	fn on_message(&self, command: &Command, payload: &Bytes) -> ProtocolResult {
		ProtocolResult::None
	}
}

