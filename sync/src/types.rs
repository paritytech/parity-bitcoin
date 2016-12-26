use std::sync::Arc;
use futures::BoxFuture;
use parking_lot::RwLock;
use db;
use local_node::LocalNode;
use miner::MemoryPool;
use synchronization_client::SynchronizationClient;
use synchronization_executor::LocalSynchronizationTaskExecutor;
use synchronization_peers::Peers;
use synchronization_server::ServerImpl;
use synchronization_verifier::AsyncVerifier;
use utils::SynchronizationState;

pub use utils::BlockHeight;

/// Network request id
pub type RequestId = u32;

/// Peer is indexed using this type
pub type PeerIndex = usize;

// No-error, no-result future
pub type EmptyBoxFuture = BoxFuture<(), ()>;

/// Reference to storage
pub type StorageRef = db::SharedStore;

/// Reference to memory pool
pub type MemoryPoolRef = Arc<RwLock<MemoryPool>>;

/// Shared synchronization state reference
pub type SynchronizationStateRef = Arc<SynchronizationState>;

/// Reference to peers
pub type PeersRef = Arc<Peers>;

/// Reference to synchronization tasks executor
pub type ExecutorRef<T> = Arc<T>;

/// Reference to synchronization client
pub type ClientRef<T> = Arc<T>;

/// Reference to synchronization server
pub type ServerRef<T> = Arc<T>;

/// Reference to local node
pub type LocalNodeRef = Arc<LocalNode<LocalSynchronizationTaskExecutor, ServerImpl, SynchronizationClient<LocalSynchronizationTaskExecutor, AsyncVerifier>>>;
