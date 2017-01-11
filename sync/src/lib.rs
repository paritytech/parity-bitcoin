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

mod blocks_writer;
mod inbound_connection;
mod inbound_connection_factory;
mod local_node;
mod synchronization_chain;
mod synchronization_client;
mod synchronization_client_core;
mod synchronization_executor;
mod synchronization_manager;
mod synchronization_peers;
mod synchronization_peers_tasks;
mod synchronization_server;
mod synchronization_verifier;
mod types;
mod utils;

pub use types::LocalNodeRef;
pub use types::PeersRef;

use std::sync::Arc;
use parking_lot::RwLock;
use network::Magic;
use primitives::hash::H256;
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

/// Synchronization events listener
pub trait SyncListener: Send + 'static {
	/// Called when node switches to synchronization state
	fn synchronization_state_switched(&self, is_synchronizing: bool);
	/// Called when new best storage block is inserted
	fn best_storage_block_inserted(&self, block_hash: &H256);
}

/// Create blocks writer.
pub fn create_sync_blocks_writer(db: db::SharedStore, network: Magic, verification: bool) -> blocks_writer::BlocksWriter {
	blocks_writer::BlocksWriter::new(db, network, verification)
}

/// Create synchronization peers
pub fn create_sync_peers() -> PeersRef {
	use synchronization_peers::PeersImpl;

	Arc::new(PeersImpl::default())
}

/// Creates local sync node for given `db`
pub fn create_local_sync_node(network: Magic, db: db::SharedStore, peers: PeersRef) -> LocalNodeRef {
	use miner::MemoryPool;
	use synchronization_chain::Chain as SyncChain;
	use synchronization_executor::LocalSynchronizationTaskExecutor as SyncExecutor;
	use local_node::LocalNode as SyncNode;
	use synchronization_server::ServerImpl;
	use synchronization_client::SynchronizationClient;
	use synchronization_client_core::{SynchronizationClientCore, CoreVerificationSink, Config as SynchronizationConfig};
	use synchronization_verifier::AsyncVerifier;
	use utils::SynchronizationState;
	use types::SynchronizationStateRef;

	let sync_client_config = SynchronizationConfig {
		network: network,
		// during regtests, peer is providing us with bad blocks => we shouldn't close connection because of this
		close_connection_on_bad_block: network != Magic::Regtest,
		// TODO: remove me
		threads_num: 4,
	};

	let memory_pool = Arc::new(RwLock::new(MemoryPool::new()));
	let sync_state = SynchronizationStateRef::new(SynchronizationState::with_storage(db.clone()));
	let sync_chain = SyncChain::new(db.clone(), memory_pool.clone());
	let chain_verifier = Arc::new(ChainVerifier::new(db.clone(), network));
	let sync_executor = SyncExecutor::new(peers.clone());
	let sync_server = Arc::new(ServerImpl::new(peers.clone(), db.clone(), memory_pool.clone(), sync_executor.clone()));
	let sync_client_core = SynchronizationClientCore::new(sync_client_config, sync_state.clone(), peers.clone(), sync_executor.clone(), sync_chain, chain_verifier.clone());
	let verifier_sink = Arc::new(CoreVerificationSink::new(sync_client_core.clone()));
	let verifier = AsyncVerifier::new(chain_verifier, db.clone(), memory_pool.clone(), verifier_sink);
	let sync_client = SynchronizationClient::new(sync_state.clone(), sync_client_core, verifier);
	Arc::new(SyncNode::new(network, db, memory_pool, peers, sync_state, sync_executor, sync_client, sync_server))
}

/// Create inbound synchronization connections factory for given local sync node.
pub fn create_sync_connection_factory(peers: PeersRef, local_sync_node: LocalNodeRef) -> p2p::LocalSyncNodeRef {
	use inbound_connection_factory::InboundConnectionFactory as SyncConnectionFactory;

	SyncConnectionFactory::new(peers, local_sync_node).boxed()
}
