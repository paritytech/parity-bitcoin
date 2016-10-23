use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Command, types};
use protocol::{Protocol, Direction};
use p2p::Context;
use PeerId;

pub struct AddrProtocol {
	/// Context
	context: Arc<Context>,
	/// Connected peer id.
	peer: PeerId,
}

impl AddrProtocol {
	pub fn new(context: Arc<Context>, peer: PeerId) -> Self {
		AddrProtocol {
			context: context,
			peer: peer,
		}
	}
}

impl Protocol for AddrProtocol {
	fn initialize(&mut self, direction: Direction, _version: u32) -> Result<(), Error> {
		// TODO: if need new peers
		if let Direction::Outbound = direction {
			let send = Context::send_to_peer(self.context.clone(), self.peer, &types::GetAddr);
			self.context.spawn(send);
		}
		Ok(())
	}

	fn on_message(&self, _command: &Command, _payload: &Bytes, _version: u32) -> Result<(), Error> {
		Ok(())
	}
}
