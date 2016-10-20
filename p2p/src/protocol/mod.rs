mod ping;

use bytes::Bytes;
use message::Error;
use message::common::Command;

pub use self::ping::PingProtocol;

pub enum Direction {
	Inbound,
	Outbound,
}

pub enum ProtocolAction {
	Reply(Bytes),
	None,
	Disconnect,
}

pub trait Protocol: Send {
	/// Initialize the protocol.
	fn initialize(&mut self, _direction: Direction, _version: u32) -> Result<ProtocolAction, Error> {
		Ok(ProtocolAction::None)
	}

	/// Handle the message.
	fn on_message(&self, command: &Command, payload: &Bytes, version: u32) -> Result<ProtocolAction, Error>;
}
