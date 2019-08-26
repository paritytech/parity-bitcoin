use std::sync::Arc;
use futures::Future;
use parking_lot::{Mutex, RwLock};
use storage;
use local_node::LocalNode;
use miner::MemoryPool;
use super::SyncListener;
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
pub type EmptyBoxFuture = Box<dyn Future<Item=(), Error=()> + Send>;

/// Reference to storage
pub type StorageRef = storage::SharedStore;

/// Reference to memory pool
pub type MemoryPoolRef = Arc<RwLock<MemoryPool>>;

/// Shared synchronization state reference
pub type SynchronizationStateRef = Arc<SynchronizationState>;

/// Reference to peers
pub type PeersRef = Arc<dyn Peers>;

/// Reference to synchronization tasks executor
pub type ExecutorRef<T> = Arc<T>;

/// Reference to synchronization client
pub type ClientRef<T> = Arc<T>;

/// Reference to synchronization client core
pub type ClientCoreRef<T> = Arc<Mutex<T>>;

/// Reference to synchronization server
pub type ServerRef<T> = Arc<T>;

/// Reference to local node
pub type LocalNodeRef = Arc<LocalNode<ServerImpl, SynchronizationClient<LocalSynchronizationTaskExecutor, AsyncVerifier>>>;

/// Synchronization events listener reference
pub type SyncListenerRef = Box<dyn SyncListener>;
