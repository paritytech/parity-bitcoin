use std::sync::Arc;
use parking_lot::RwLock;
use db::SharedStore;
use miner::MemoryPool;

/// Local peer index
pub const LOCAL_PEER_INDEX: PeerIndex = 0;

/// Peers are indexed by this type
pub type PeerIndex = usize;

/// Requests IDs
pub type RequestId = u32;

/// Block height type
pub type BlockHeight = u32;

/// Synchronization peers reference
pub type PeersRef<T> = Arc<T>;

/// Synchronization client reference
pub type ClientRef<T> = Arc<T>;

/// Synchronization executor reference
pub type ExecutorRef<T> = Arc<T>;

/// Memory pool reference
pub type MemoryPoolRef = Arc<RwLock<MemoryPool>>;

/// Storage reference
pub type StorageRef = SharedStore;
