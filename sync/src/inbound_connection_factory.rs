use std::sync::atomic::{AtomicUsize, Ordering};
use p2p::{LocalSyncNode, LocalSyncNodeRef, OutboundSyncConnectionRef, InboundSyncConnectionRef};
use message::Services;
use inbound_connection::InboundConnection;
use types::{PeersRef, LocalNodeRef};

/// Inbound synchronization connection factory
pub struct InboundConnectionFactory {
	/// Peers reference
	peers: PeersRef,
	/// Reference to synchronization node
	node: LocalNodeRef,
	/// Throughout counter of synchronization peers
	counter: AtomicUsize,
}

impl InboundConnectionFactory {
	/// Create new inbound connection factory
	pub fn new(peers: PeersRef, node: LocalNodeRef) -> Self {
		InboundConnectionFactory {
			peers: peers,
			node: node,
			counter: AtomicUsize::new(0),
		}
	}

	/// Box inbound connection factory
	pub fn boxed(self) -> LocalSyncNodeRef {
		Box::new(self)
	}
}

impl LocalSyncNode for InboundConnectionFactory {
	fn create_sync_session(&self, _best_block_height: i32, services: Services, outbound_connection: OutboundSyncConnectionRef) -> InboundSyncConnectionRef {
		let peer_index = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
		trace!(target: "sync", "Creating new sync session with peer#{}", peer_index);
		// remember outbound connection
		self.peers.insert(peer_index, services, outbound_connection);
		// create new inbound connection
		InboundConnection::new(peer_index, self.peers.clone(), self.node.clone()).boxed()
	}
}
