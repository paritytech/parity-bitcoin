mod addr;
mod ping;
mod sync;

use bytes::Bytes;
use message::Error;
use message::common::Command;

pub use self::addr::AddrProtocol;
pub use self::ping::PingProtocol;
pub use self::sync::{SyncProtocol, InboundSyncConnection, InboundSyncConnectionRef, OutboundSyncConnection, OutboundSyncConnectionRef, LocalSyncNode, LocalSyncNodeRef};

#[derive(PartialEq, Clone, Copy)]
pub enum Direction {
	Inbound,
	Outbound,
}

pub trait Protocol: Send {
	/// Initialize the protocol.
	fn initialize(&mut self, _direction: Direction, _version: u32) -> Result<(), Error> {
		Ok(())
	}

	/// Handle the message.
	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error>;

	/// Boxes the protocol.
	fn boxed(self) -> Box<Protocol> where Self: Sized + 'static {
		Box::new(self)
	}
}
