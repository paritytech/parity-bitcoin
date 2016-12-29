use primitives::hash::H256;
use parking_lot::RwLock;
use storage::Storage;
use transaction_meta::TransactionMeta;
use std::collections::{HashMap, HashSet};
use linked_hash_map::LinkedHashMap;
use chain::{IndexedBlock, IndexedTransaction};
use block_stapler::BlockInsertedChain;
use transaction_provider::TransactionProvider;
use transaction_meta_provider::TransactionMetaProvider;

pub enum TransactionParentLocation {
	// Parent transaction is in the same block as current transaction or in database
	DatabaseOrSameBlock,
	// Parent transaction is in the one of the block upper in queue
	Upstream(H256),
	// Parent transaction is in the one of the block upper in queue and is verified
	PendingUpstream(H256),
}

pub type PreparedTransactions = HashMap<H256, TransactionParentLocation>;

pub struct PreparedBlock {
	block: IndexedBlock,
	transactions: PreparedTransactions,
}

pub struct VerifiedBlock {
	block: IndexedBlock,
	transactions: PreparedTransactions,
	location: BlockInsertedChain,
}

// locks are aquired in the order in the struct
pub struct BlockQueue {
	// location of transactions which are still in queued blocks, with the block hash locator
	pending_transactions: RwLock<HashMap<H256, H256>>,
	// location of transactions which are in verified blocks, with the block hash locator
	verified_transactions: RwLock<HashMap<H256, H256>>,
	// unspent overlay with updates caused by verified blocks
	unspent_overlay: RwLock<HashMap<H256, TransactionMeta>>,
	// inserted blocks
	added: RwLock<LinkedHashMap<H256, IndexedBlock>>,
	// blocks prepared for verification
	unverified: RwLock<LinkedHashMap<H256, PreparedBlock>>,
	// in process
	processing: RwLock<HashSet<H256>>,
	// verified blocks
	verified: RwLock<LinkedHashMap<H256, PreparedBlock>>,
	// invalid blocks
	invalid: RwLock<HashSet<H256>>,
}

enum TaskResult { Ok, Wait }

struct BlockQueueSummary {
	added: usize,
	unverified: usize,
	verified: usize,
}

impl BlockQueue {
	// new queue

	pub fn new() -> BlockQueue {
		BlockQueue {
			pending_transactions: Default::default(),
			verified_transactions: Default::default(),
			unspent_overlay: Default::default(),
			added: Default::default(),
			unverified: Default::default(),
			processing: Default::default(),
			verified: Default::default(),
			invalid: Default::default(),
		}
	}

	pub fn push(&self, block: IndexedBlock) {
		self.added.write().insert(block.hash().clone(), block);
	}

	pub fn summary(&self) -> BlockQueueSummary {
		BlockQueueSummary {
			added: self.added.read().len(),
			unverified: self.unverified.read().len(),
			verified: self.verified.read().len(),
		}
	}

	// converts added indexed block into block prepared for verification
	pub fn fetch(&self, db: &Storage) -> TaskResult {
		let block: IndexedBlock = {
			let mut added_lock = self.added.write();
			let mut processing_lock = self.processing.write();
			let (hash, block) = match added_lock.pop_front() {
				None => { return TaskResult::Wait },
				Some(b) => b,
			};
			processing_lock.insert(hash);
			block
		};

		let mut prepared_txes = PreparedTransactions::new();

		for block_tx in block.transactions.iter() {
			for input in block_tx.raw.inputs.iter().skip(1) {
				let parent_tx_hash = &input.previous_output.hash;

				if let Some(block_locator) = self.pending_transactions.read().get(parent_tx_hash) {
					prepared_txes.insert(parent_tx_hash.clone(), TransactionParentLocation::PendingUpstream(block_locator.clone()));
				} else if let Some(block_locator) = self.verified_transactions.read().get(parent_tx_hash) {
					prepared_txes.insert(parent_tx_hash.clone(), TransactionParentLocation::Upstream(block_locator.clone()));
				} else {
					// this will put transaction parent and meta in lru cache
					// todo: maybe use local cache
					// todo: maybe scan block first (depends on how often transactions reference parents in the same block)
					db.transaction(parent_tx_hash);
					db.transaction_meta(parent_tx_hash);

					prepared_txes.insert(parent_tx_hash.clone(), TransactionParentLocation::DatabaseOrSameBlock);
				}
			}
		}

		{
			let mut unverified = self.unverified.write();
			let mut processing = self.processing.write();
			let mut pending_txes = self.pending_transactions.write();

			for tx in block.transactions.iter() {
				pending_txes.insert(tx.hash.clone(), block.hash().clone());
			}

			processing.remove(block.hash());
			unverified.insert(block.hash().clone(),
				PreparedBlock {
					block: block,
					transactions: prepared_txes,
				}
			);
		}

		TaskResult::Ok
	}
}

#[cfg(test)]
mod tests {

	use super::BlockQueue;
	use storage::Storage;
	use devtools::RandomTempPath;
	use test_data;

	#[test]
	fn push() {
		let queue = BlockQueue::new();

		queue.push(test_data::block_h2().into());

		assert_eq!(queue.summary().added, 1);
	}

	#[test]
	fn fetch() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();
		let queue = BlockQueue::new();
		queue.push(test_data::block_h2().into());

		queue.fetch(&store);

		assert_eq!(queue.summary().unverified, 1);
		assert_eq!(queue.summary().added, 0);
	}
}
