extern crate bitcrypto;
extern crate byteorder;
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
extern crate bit_vec;
extern crate murmur3;
extern crate primitives;
extern crate test_data;
extern crate time;
extern crate verification;
extern crate miner;
extern crate script;
extern crate serialization as ser;
#[cfg(test)]
extern crate ethcore_devtools as devtools;
extern crate rand;
extern crate network;

mod best_headers_chain;
mod blocks_writer;
mod compact_block_builder;
mod connection_filter;
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
mod synchronization_verifier;

pub use local_node::LocalNodeRef;

use std::sync::Arc;
use parking_lot::RwLock;
use tokio_core::reactor::Handle;
use network::Magic;
use verification::BackwardsCompatibleChainVerifier as ChainVerifier;

/// Sync errors.
#[derive(Debug)]
pub enum Error {
	/// Too many orphan blocks.
	TooManyOrphanBlocks,
	/// Database error.
	Database(db::Error),
	/// Block verification error.
	Verification(String),
}

/// Create blocks writer.
pub fn create_sync_blocks_writer(db: db::SharedStore, network: Magic) -> blocks_writer::BlocksWriter {
	blocks_writer::BlocksWriter::new(db, network)
}

/// Creates local sync node for given `db`
pub fn create_local_sync_node(handle: &Handle, network: Magic, db: db::SharedStore) -> LocalNodeRef {
	use synchronization_chain::Chain as SyncChain;
	use synchronization_executor::LocalSynchronizationTaskExecutor as SyncExecutor;
	use local_node::LocalNode as SyncNode;
	use synchronization_server::SynchronizationServer;
	use synchronization_client::{SynchronizationClient, SynchronizationClientCore, CoreVerificationSink, Config as SynchronizationConfig};
	use synchronization_verifier::AsyncVerifier;

	let sync_client_config = SynchronizationConfig {
		network: network,
		// during regtests, peer is providing us with bad blocks => we shouldn't close connection because of this
		close_connection_on_bad_block: network != Magic::Regtest,
		// TODO: remove me
		threads_num: 4,
	};

	let sync_chain = Arc::new(RwLock::new(SyncChain::new(db.clone())));
	let chain_verifier = Arc::new(ChainVerifier::new(db, network));
	let sync_executor = SyncExecutor::new(sync_chain.clone());
	let sync_server = Arc::new(SynchronizationServer::new(sync_chain.clone(), sync_executor.clone()));
	let sync_client_core = SynchronizationClientCore::new(sync_client_config, handle, sync_executor.clone(), sync_chain.clone(), chain_verifier.clone());
	let verifier_sink = Arc::new(CoreVerificationSink::new(sync_client_core.clone()));
	let verifier = AsyncVerifier::new(chain_verifier, sync_chain, verifier_sink);
	let sync_client = SynchronizationClient::new(sync_client_core, verifier);
	Arc::new(SyncNode::new(sync_server, sync_client, sync_executor))
}

/// Create inbound synchronization connections factory for given local sync node.
pub fn create_sync_connection_factory(local_sync_node: LocalNodeRef) -> p2p::LocalSyncNodeRef {
	use inbound_connection_factory::InboundConnectionFactory as SyncConnectionFactory;

	SyncConnectionFactory::with_local_node(local_sync_node)
}
