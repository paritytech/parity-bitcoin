mod addr;
mod ping;
mod sync;

use bytes::Bytes;
use message::Error;
use message::common::Command;

pub use self::addr::{AddrProtocol, SeednodeProtocol};
pub use self::ping::PingProtocol;
pub use self::sync::{SyncProtocol,
	InboundSyncConnection, InboundSyncConnectionRef,
	InboundSyncConnectionState, InboundSyncConnectionStateRef,
	OutboundSyncConnection, OutboundSyncConnectionRef,
	LocalSyncNode, LocalSyncNodeRef,
};

pub trait Protocol: Send {
	/// Initialize the protocol.
	fn initialize(&mut self) {}

	/// Maintain the protocol.
	fn maintain(&mut self) {}

	/// Handle the message.
	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error>;

	/// On disconnect.
	fn on_close(&mut self) {}

	/// Boxes the protocol.
	fn boxed(self) -> Box<dyn Protocol> where Self: Sized + 'static {
		Box::new(self)
	}
}
