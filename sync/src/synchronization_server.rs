use std::sync::Arc;
use parking_lot::Mutex;
use db;
use primitives::hash::H256;
use synchronization_chain::ChainRef;
use synchronization_executor::{Task, TaskExecutor};

/// Synchronization requests server
pub struct Server<T> where T: TaskExecutor + Send + 'static {
	chain: ChainRef,
	executor: Arc<Mutex<T>>,
}

impl<T> Server<T> where T: TaskExecutor + Send + 'static {
	pub fn new(chain: ChainRef, executor: Arc<Mutex<T>>) -> Self {
		Server {
			chain: chain,
			executor: executor,
		}
	}

	pub fn serve_block(&self, peer_index: usize, block_hash: H256) {
		if let Some(block) = self.chain.read().storage_block(&block_hash) {
			self.executor.lock().execute(Task::SendBlock(peer_index, block));
		}
	}

	pub fn serve_blocks_inventory(&self, _peer_index: usize, _best_common_block: db::BestBlock) {
		unimplemented!();
	}

	pub fn serve_blocks_headers(&self, _peer_index: usize, _best_common_block: db::BestBlock) {
		unimplemented!();
	}
}
