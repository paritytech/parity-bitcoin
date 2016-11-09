use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use message::{Error, Command, deserialize_payload, Payload};
use message::types::{GetAddr, Addr};
use protocol::Protocol;
use p2p::Context;
use util::{Direction, PeerInfo};

pub struct AddrProtocol {
	/// Context
	context: Arc<Context>,
	/// Connected peer info.
	info: PeerInfo,
}

impl AddrProtocol {
	pub fn new(context: Arc<Context>, info: PeerInfo) -> Self {
		AddrProtocol {
			context: context,
			info: info,
		}
	}
}

impl Protocol for AddrProtocol {
	fn initialize(&mut self) {
		if let Direction::Outbound = self.info.direction {
			let send = Context::send_to_peer(self.context.clone(), self.info.id, &GetAddr);
			self.context.spawn(send);
		}
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		// normal nodes send addr message only after they receive getaddr message
		// meanwhile seednodes, surprisingly, send addr message even before they are asked for it
		if command == &GetAddr::command() {
			let _: GetAddr = try!(deserialize_payload(payload, self.info.version));
			let entries = self.context.node_table_entries().into_iter().map(Into::into).collect();
			let addr = Addr::new(entries);
			let send = Context::send_to_peer(self.context.clone(), self.info.id, &addr);
			self.context.spawn(send);
		} else if command == &Addr::command() {
			let addr: Addr = try!(deserialize_payload(payload, self.info.version));
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

pub struct SeednodeProtocol {
	/// Context
	context: Arc<Context>,
	/// Connected peer info,
	info: PeerInfo,
	/// Indicates if disconnecting has been scheduled.
	disconnecting: bool,
}

impl SeednodeProtocol {
	pub fn new(context: Arc<Context>, info: PeerInfo) -> Self {
		SeednodeProtocol {
			context: context,
			info: info,
			disconnecting: false,
		}
	}
}

impl Protocol for SeednodeProtocol {
	fn on_message(&mut self, command: &Command, _payload: &Bytes) -> Result<(), Error> {
		// Seednodes send addr message more than once with different addresses.
		// We can't disconenct after first read. Let's delay it by 60 seconds.
		if !self.disconnecting && command == &Addr::command() {
			self.disconnecting = true;
			let context = self.context.clone();
			let peer = self.info.id;
			self.context.execute_after(Duration::new(60, 0), move || {
				context.close_channel(peer);
			});
		}
		Ok(())
	}
}
