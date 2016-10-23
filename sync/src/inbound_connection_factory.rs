use std::sync::Arc;
use parking_lot::Mutex;
use p2p::protocol::sync::{LocalSyncNode, OutboundSyncConnectionRef, InboundSyncConnectionRef};
use local_node::LocalNodeRef;
use inbound_connection::InboundConnection;

pub struct InboundConnectionFactory {
	local_node: LocalNodeRef,
}

impl InboundConnectionFactory {
	pub fn with_local_node(local_node: LocalNodeRef) -> Arc<Mutex<Box<LocalSyncNode>>> {
		Arc::new(
			Mutex::new(
				Box::new(
					InboundConnectionFactory {
						local_node: local_node,
					}
				)
			)
		)
	}
}

impl LocalSyncNode for InboundConnectionFactory {
	fn start_height(&self) -> i32 {
		self.local_node.lock().best_block().height as i32 // TODO: why i32 in protocol?
	}

	fn start_sync_session(&mut self, best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> InboundSyncConnectionRef {
		// TODO: drop connections to peers with version < 31800 (getheaders version)
		let peer_index = self.local_node.lock().start_sync_session(best_block_height, outbound_connection);
		let inbound_connection = InboundConnection::new(self.local_node.clone(), peer_index);
		inbound_connection
	}
}