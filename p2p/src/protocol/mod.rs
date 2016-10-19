mod ping;

use bytes::Bytes;
use message::Error;
use message::common::Command;

pub use self::ping::PingProtocol;

pub enum ProtocolResult {
	Reply(Bytes),
	Error(Error),
	None,
	Disconnect,
}

pub trait Protocol {
	/// Initialize the protocol.
	fn initialize(&self);
	/// Handle the message.
	fn on_message(&self, command: &Command, payload: &Bytes) -> ProtocolResult;
}
