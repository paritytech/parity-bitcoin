use std::sync::Arc;
use std::time::Duration;
use bytes::Bytes;
use message::{Error, Command, deserialize_payload, Payload};
use message::types::{GetAddr, Addr};
use protocol::Protocol;
use net::PeerContext;
use util::Direction;

pub struct AddrProtocol {
	/// Context
	context: Arc<PeerContext>,
	/// True if this is a connection to the seednode && we should disconnect after receiving addr message
	is_seed_node_connection: bool,
}

impl AddrProtocol {
	pub fn new(context: Arc<PeerContext>, is_seed_node_connection: bool) -> Self {
		AddrProtocol {
			context: context,
			is_seed_node_connection: is_seed_node_connection,
		}
	}
}

impl Protocol for AddrProtocol {
	fn initialize(&mut self) {
		if let Direction::Outbound = self.context.info().direction {
			self.context.send_request(&GetAddr);
		}
	}

	fn on_message(&mut self, command: &Command, payload: &Bytes) -> Result<(), Error> {
		// normal nodes send addr message only after they receive getaddr message
		// meanwhile seednodes, surprisingly, send addr message even before they are asked for it
		if command == &GetAddr::command() {
			let _: GetAddr = try!(deserialize_payload(payload, self.context.info().version));
			let entries = self.context.global().node_table_entries().into_iter().map(Into::into).collect();
			let addr = Addr::new(entries);
			self.context.send_response_inline(&addr);
		} else if command == &Addr::command() {
			let addr: Addr = try!(deserialize_payload(payload, self.context.info().version));
			match addr {
				Addr::V0(_) => {
					unreachable!("This version of protocol is not supported!");
				},
				Addr::V31402(addr) => {
					let nodes_len = addr.addresses.len();
					self.context.global().update_node_table(addr.addresses);
					// seednodes are currently responding with two addr messages:
					// 1) addr message with single address - seednode itself
					// 2) addr message with 1000 addresses (seednode node_table contents)
					if self.is_seed_node_connection && nodes_len > 1 {
						self.context.close();
					}
				},
			}
		}
		Ok(())
	}
}

pub struct SeednodeProtocol {
	/// Context
	context: Arc<PeerContext>,
	/// Indicates if disconnecting has been scheduled.
	disconnecting: bool,
}

impl SeednodeProtocol {
	pub fn new(context: Arc<PeerContext>) -> Self {
		SeednodeProtocol {
			context: context,
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
			let context = self.context.global().clone();
			let peer = self.context.info().id;
			self.context.global().execute_after(Duration::new(60, 0), move || {
				context.close_channel(peer);
			});
		}
		Ok(())
	}
}
