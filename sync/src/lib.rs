extern crate chain;
extern crate db;
#[macro_use]
extern crate log;
extern crate futures;
extern crate futures_cpupool;
extern crate tokio_core;
extern crate message;
extern crate p2p;
extern crate parking_lot;
extern crate linked_hash_map;
extern crate primitives;
extern crate test_data;
extern crate time;
extern crate verification;
extern crate miner;
#[cfg(test)]
extern crate ethcore_devtools as devtools;

mod best_headers_chain;
mod blocks_writer;
mod hash_queue;
mod inbound_connection;
mod inbound_connection_factory;
mod local_node;
mod orphan_blocks_pool;
mod orphan_transactions_pool;
mod synchronization_chain;
mod synchronization_client;
mod synchronization_executor;
mod synchronization_manager;
mod synchronization_peers;
mod synchronization_server;

use std::sync::Arc;
use parking_lot::RwLock;
use tokio_core::reactor::Handle;
use message::common::ConsensusParams;

/// Sync errors.
#[derive(Debug)]
pub enum Error {
	/// Out of order block.
	OutOfOrderBlock,
	/// Database error.
	Database(db::Error),
	/// Block verification error.
	Verification(verification::Error),
}

/// Create blocks writer.
pub fn create_sync_blocks_writer(db: Arc<db::Store>) -> blocks_writer::BlocksWriter {
	blocks_writer::BlocksWriter::new(db)
}

/// Create inbound synchronization connections factory for given `db`.
pub fn create_sync_connection_factory(handle: &Handle, consensus_params: ConsensusParams, db: Arc<db::Store>) -> p2p::LocalSyncNodeRef {
	use synchronization_chain::Chain as SyncChain;
	use synchronization_executor::LocalSynchronizationTaskExecutor as SyncExecutor;
	use local_node::LocalNode as SyncNode;
	use inbound_connection_factory::InboundConnectionFactory as SyncConnectionFactory;
	use synchronization_server::SynchronizationServer;
	use synchronization_client::{SynchronizationClient, Config as SynchronizationConfig};

	let sync_chain = Arc::new(RwLock::new(SyncChain::new(db)));
	let sync_executor = SyncExecutor::new(sync_chain.clone());
	let sync_server = Arc::new(SynchronizationServer::new(sync_chain.clone(), sync_executor.clone()));
	let sync_client = SynchronizationClient::new(SynchronizationConfig::with_consensus_params(consensus_params), handle, sync_executor.clone(), sync_chain);
	let sync_node = Arc::new(SyncNode::new(sync_server, sync_client, sync_executor));
	SyncConnectionFactory::with_local_node(sync_node)
}
