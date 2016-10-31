extern crate chain;
extern crate db;
#[macro_use]
extern crate log;
extern crate message;
extern crate p2p;
extern crate parking_lot;
extern crate primitives;
extern crate time;
extern crate verification;

mod hash_queue;
mod inbound_connection;
mod inbound_connection_factory;
mod local_node;
mod synchronization;
mod synchronization_chain;
mod synchronization_executor;
mod synchronization_peers;

use std::sync::Arc;
use parking_lot::RwLock;

/// Create inbound synchronization connections factory for given `db`.
pub fn create_sync_connection_factory(db: Arc<db::Store>) -> p2p::LocalSyncNodeRef {
	use synchronization_chain::Chain as SyncChain;
	use synchronization_executor::LocalSynchronizationTaskExecutor as SyncExecutor;
	use local_node::LocalNode as SyncNode;
	use inbound_connection_factory::InboundConnectionFactory as SyncConnectionFactory;

	let sync_chain = Arc::new(RwLock::new(SyncChain::new(db)));
	let sync_executor = SyncExecutor::new(sync_chain.clone());
	let sync_node = SyncNode::new(sync_chain, sync_executor);
	SyncConnectionFactory::with_local_node(sync_node)
}
