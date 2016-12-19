use std::collections::HashSet;
use chain::{IndexedTransaction, IndexedBlockHeader, IndexedBlock};
use db::BestBlock;
use primitives::hash::H256;

/// Index of 'verifying' queue
const VERIFYING_QUEUE: usize = 0;
/// Index of 'requested' queue
const REQUESTED_QUEUE: usize = 1;
/// Index of 'scheduled' queue
const SCHEDULED_QUEUE: usize = 2;
/// Number of hash queues
const NUMBER_OF_QUEUES: usize = 3;

/// Block synchronization state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum BlockState {
	/// Block is unknown
	Unknown,
	/// Block is orphan
	Orphan,
	/// This block has been marked as dead-end block
	DeadEnd,
	/// Scheduled for requesting
	Scheduled,
	/// Requested from peers
	Requested,
	/// Currently verifying
	Verifying,
	/// In storage
	Stored,
}

/// Block insertion result
#[derive(Debug, Default, PartialEq)]
pub struct BlockInsertionResult {
	/// Hashes of blocks, which were canonized during this insertion procedure. Order matters
	pub canonized_blocks_hashes: Vec<H256>,
	/// Transaction to 'reverify'. Order matters
	pub transactions_to_reverify: Vec<IndexedTransaction>,
}

/// Synchronization chain information
pub struct Information {
	/// Number of blocks hashes currently scheduled for requesting
	pub scheduled: BlockHeight,
	/// Number of blocks hashes currently requested from peers
	pub requested: BlockHeight,
	/// Number of blocks currently verifying
	pub verifying: BlockHeight,
	/// Number of blocks in the storage
	pub stored: BlockHeight,
	/// Information on memory pool
	pub transactions: MemoryPoolInformation,
	/// Information on headers chain
	pub headers: BestHeadersInformation,
}

/// Blockchain blocks from synchroniation point of view, consisting of:
/// 1) all blocks from the `storage` [oldest blocks]
/// 2) all blocks currently verifying by `synchronization_verifier`
/// 3) all blocks currently requested from peers
/// 4) all blocks currently scheduled for requesting [newest blocks]
pub trait BlocksQueue {
	/// Returns queue information
	fn information(&self) -> Information;
	/// Returns state of given block
	fn state(&self, hash: &H256) -> BlockState;
	/// Insert dead-end block
	fn dead_end_block(&mut self, hash: H256);
	/// Schedule blocks for retrieval
	fn schedule(&mut self, headers: &[IndexedBlockHeader]);
	/// Forget block
	fn forget(&mut self, hash: &H256);
	/// Forget block, but leave header
	fn forget_leave_header(&mut self, hash: &H256) -> HashPosition;
	/// Append blocks header to verification queue
	fn verify(&mut self, header: IndexedBlockHeader);
	/// Remove orphan blocks for header
	fn remove_blocks_for_parent(&mut self, hash: &H256) -> Vec<IndexedBlock>;
/*


	/// Returns best known block information
	fn best_block() -> BestBlock;
	/// Returns number of blocks with given state
	fn state_length(&self, state: BlockState) -> BlockHeight;
	/// Returns `n` best blocks of the given state
	fn front_n_of_state(&self, state: BlockState, n: BlockHeight) -> Vec<H256>;
	/// Returns block locator hashes
	fn block_locator_hashes(&self) -> Vec<H256>;
	/// Schedule blocks headers for retrieval
	fn schedule_blocks_headers(&mut self, headers: Vec<IndexedBlockHeader>);
	/// Request `n` blocks headers
	fn request_blocks_hashes(&mut self, n: u32) -> Vec<H256>;
	/// Append blocks header to verification queue
	fn verify_block(&mut self, header: IndexedBlockHeader);
	/// Insert verified block to storage
	fn insert_verified_block(&mut self, block: IndexedBlock) -> Result<BlockInsertionResult, DbError>;
	/// Forget block hash
	fn forget(&mut self, hash: &H256) -> HashPosition;
	/// Forget block hash && header
	fn forget_leave_header(&mut self, hash: &H256) -> HashPosition;
	/// Forget block with children
	fn forget_with_children(&mut self, hash: &H256);*/
}

/// Blocks queue implementation
pub struct TransactionsQueueImpl {
	/// Genesis block hash
	genesis_block_hash: H256,
	/// Best storage block (stored for optimizations)
	best_storage_block: BestBlock,
	/// Dead-end blocks
	dead_end: HashSet<H256>,
	/// Unknown blocks
	unknown_blocks: UnknownBlocksPool,
	/// Orphaned blocks
	orphan_pool: OrphanBlocksPool,
	/// Hashes chain
	hash_queue: HashQueueChain,
	/// Headers chain
	headers_queue: BlockHeaderChain,
	/// Storage reference
	storage: StorageRef,
}
