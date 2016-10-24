use std::sync::Arc;
use bytes::Bytes;
use message::{Error, Command, deserialize_payload, Payload};
use message::types::{GetAddr, Addr};
use protocol::{Protocol, Direction};
use p2p::Context;
use PeerId;

pub struct AddrProtocol {
	/// Context
	context: Arc<Context>,
	/// Connected peer id.
	peer: PeerId,
	/// True if expect addr message.
	expects_addr: bool,
}

impl AddrProtocol {
	pub fn new(context: Arc<Context>, peer: PeerId) -> Self {
		AddrProtocol {
			context: context,
			peer: peer,
			expects_addr: false,
		}
	}
}

impl Protocol for AddrProtocol {
	fn initialize(&mut self, direction: Direction, _version: u32) {
		if let Direction::Outbound = direction {
			self.expects_addr = true;
			let send = Context::send_to_peer(self.context.clone(), self.peer, &GetAddr);
			self.context.spawn(send);
		}
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes, version: u32) -> Result<(), Error> {
		if command == &GetAddr::command() {
			let _: GetAddr = try!(deserialize_payload(payload, version));
			let entries = self.context.node_table_entries().into_iter().map(Into::into).collect();
			let addr = Addr::new(entries);
			let send = Context::send_to_peer(self.context.clone(), self.peer, &addr);
			self.context.spawn(send);
		} else if command == &Addr::command() {
			let addr: Addr = try!(deserialize_payload(payload, version));
			match addr {
				Addr::V0(_) => {
					unreachable!("This version of protocol is not supported!");
				},
				Addr::V31402(addr) => {
					let nodes = addr.addresses.into_iter().map(Into::into).collect();
					self.context.update_node_table(nodes);
				},
			}
		}
		Ok(())
	}
}
