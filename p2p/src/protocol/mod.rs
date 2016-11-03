mod addr;
mod ping;
mod sync;

use bytes::Bytes;
use message::Error;
use message::common::Command;

use util::Direction;
pub use self::addr::{AddrProtocol, SeednodeProtocol};
pub use self::ping::PingProtocol;
pub use self::sync::{SyncProtocol, InboundSyncConnection, InboundSyncConnectionRef, OutboundSyncConnection, OutboundSyncConnectionRef, LocalSyncNode, LocalSyncNodeRef};

pub trait Protocol: Send {
	/// Initialize the protocol.
	fn initialize(&mut self, _direction: Direction, _version: u32) {}

	/// Handle the message.
	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error>;

	/// On disconnect.
	fn on_close(&mut self) {}

	/// Boxes the protocol.
	fn boxed(self) -> Box<Protocol> where Self: Sized + 'static {
		Box::new(self)
	}
}
