use primitives::hash::H256;
use parking_lot::RwLock;
use storage::Storage;
use transaction_meta::TransactionMeta;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
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
	waits_on: HashSet<H256>,
}

impl PreparedBlock {
	fn ready(&self) -> bool {
		self.waits_on.len() == 0
	}
}

pub struct BlockInsertedChainHeight {
	chain: BlockInsertedChain,
	height: u32,
}

pub struct VerifiedBlock {
	block: IndexedBlock,
	location: BlockInsertedChainHeight,
}

pub struct VerificationArtifacts {
	block_location: BlockInsertedChainHeight,
	meta_update: HashMap<H256, MetaEntry>,
}

impl VerifiedBlock {
	pub fn new(block: IndexedBlock, location: BlockInsertedChainHeight) -> Self {
		VerifiedBlock { block: block, location: location }
	}
}

pub enum MetaEntry {
	Updated(TransactionMeta),
	Removed,
}

// locks are aquired in the order in the struct
pub struct BlockQueue {
	// location of transactions which are still in queued blocks, with the block hash locator
	pending_transactions: RwLock<HashMap<H256, H256>>,
	// location of transactions which are in verified blocks, with the block hash locator
	verified_transactions: RwLock<HashMap<H256, H256>>,
	// unspent overlay with updates caused by verified blocks
	meta_overlay: RwLock<HashMap<H256, MetaEntry>>,
	// inserted blocks
	added: RwLock<LinkedHashMap<H256, IndexedBlock>>,
	// blocks prepared for verification
	unverified: RwLock<LinkedHashMap<H256, PreparedBlock>>,
	// in process
	processing: RwLock<HashSet<H256>>,
	// verified blocks
	verified: RwLock<LinkedHashMap<H256, VerifiedBlock>>,
	// invalid blocks
	invalid: RwLock<HashSet<H256>>,
}

pub enum TaskResult { Ok, Wait }

pub struct BlockQueueSummary {
	added: usize,
	unverified: usize,
	verified: usize,
}

pub trait VerifyBlock {
	type Error;

	fn verify(&self, block: &IndexedBlock) -> Result<VerificationArtifacts, Self::Error>;
}

impl BlockQueue {
	// new queue

	pub fn new() -> BlockQueue {
		BlockQueue {
			pending_transactions: Default::default(),
			verified_transactions: Default::default(),
			meta_overlay: Default::default(),
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
		let mut waits = HashSet::new();

		for block_tx in block.transactions.iter() {
			for input in block_tx.raw.inputs.iter().skip(1) {
				let parent_tx_hash = &input.previous_output.hash;


				if let Some(block_locator) = self.pending_transactions.read().get(parent_tx_hash) {
					prepared_txes.insert(parent_tx_hash.clone(), TransactionParentLocation::PendingUpstream(block_locator.clone()));
					waits.insert(block_locator.clone());
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
					waits_on: waits,
				}
			);
		}

		TaskResult::Ok
	}

	pub fn verify<V>(&self, verifier: &V) -> TaskResult where V: VerifyBlock {
		// will return first block that is ready to verify
		let block = {
			let mut unverified = self.unverified.write();
			let mut processing = self.processing.write();

			let mut ready_hash: Option<H256> = None;
			for (hash, block) in unverified.iter() {
				if block.ready() {
					ready_hash = Some(hash.clone());
				}
			}

			match ready_hash {
				None => { return TaskResult::Wait; }
				Some(hash) => {
					let block = unverified.remove(&hash).expect("We just located it above with lock is still on");
					processing.insert(hash);
					block
				}
			}
		};

		match verifier.verify(&block.block) {
			Ok(artifacts) => {
				// todo: chain notify
				{
					let verified_hash = block.block.hash().clone();
					let verified_block = block.block;

					let mut pending_txes = self.pending_transactions.write();
					let mut verified_txes = self.verified_transactions.write();
					let mut unverified = self.unverified.write();
					let mut verified = self.verified.write();

					// kick pending blocks to proceed
					for (_, mut block) in unverified.iter_mut() {
						block.waits_on.remove(&verified_hash);
					}

					// move transactions from pending to verified
					for tx in verified_block.transactions.iter() {
						if let Entry::Occupied(entry) = pending_txes.entry(tx.hash.clone()) {
							let (key, val) = entry.remove_entry();
							verified_txes.insert(key, val);
						}
					}

					verified.insert(verified_hash,
						VerifiedBlock::new(verified_block, artifacts.block_location));
				}
			},
			Err(_) => {
				// todo: chain notify
			}
		}

		TaskResult::Ok
	}
}

#[cfg(test)]
mod tests {

	use super::{BlockQueue, VerifyBlock, BlockInsertedChainHeight, VerificationArtifacts};
	use storage::Storage;
	use devtools::RandomTempPath;
	use test_data;
	use chain::IndexedBlock;
	use block_stapler::{BlockStapler, BlockInsertedChain};

	struct DummyError;
	struct FacileVerifier;

	impl VerifyBlock for FacileVerifier {
		type Error = DummyError;

		fn verify(&self, block: &IndexedBlock) -> Result<VerificationArtifacts, DummyError> {
			Ok(
				VerificationArtifacts {
					block_location: BlockInsertedChainHeight {
						chain: BlockInsertedChain::Main, height: 1
					},
					meta_update: Default::default(),
				}
			)
		}
	}

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

	#[test]
	fn verify() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_hash = genesis.hash();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let b1: IndexedBlock  = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().build()
				.build()
			.merkled_header().build()
			.build()
			.into();

		let queue = BlockQueue::new();
		queue.push(b1);
		queue.fetch(&store);

		queue.verify(&FacileVerifier);

		assert_eq!(queue.summary().verified, 1);
		assert_eq!(queue.summary().unverified, 0);
		assert_eq!(queue.summary().added, 0);
	}

}
