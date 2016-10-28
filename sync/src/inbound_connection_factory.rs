use p2p::{LocalSyncNode, LocalSyncNodeRef, OutboundSyncConnectionRef, InboundSyncConnectionRef};
use local_node::LocalNodeRef;
use inbound_connection::InboundConnection;

pub struct InboundConnectionFactory {
	local_node: LocalNodeRef,
}

impl InboundConnectionFactory {
	pub fn with_local_node(local_node: LocalNodeRef) -> LocalSyncNodeRef {
		Box::new(
			InboundConnectionFactory {
				local_node: local_node,
			}
		)
	}
}

impl LocalSyncNode for InboundConnectionFactory {
	fn start_height(&self) -> i32 {
		self.local_node.best_block().height as i32
	}

	fn create_sync_session(&self, best_block_height: i32, outbound_connection: OutboundSyncConnectionRef) -> InboundSyncConnectionRef {
		let peer_index = self.local_node.create_sync_session(best_block_height, outbound_connection);
		let inbound_connection = InboundConnection::new(self.local_node.clone(), peer_index);
		inbound_connection
	}
}