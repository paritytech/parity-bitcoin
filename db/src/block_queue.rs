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

pub struct PreparedBlock {
	block: IndexedBlock,
	waits_on: HashSet<H256>,
	depends_on: HashSet<H256>,
}

impl From<IndexedBlock> for PreparedBlock {
	fn from(block: IndexedBlock) -> Self {
		PreparedBlock {
			block: block,
			waits_on: HashSet::with_capacity(0), // much of the cases unused
			depends_on: HashSet::with_capacity(0), // much of the cases unused
		}
	}
}

impl PreparedBlock {
	fn ready(&self) -> bool {
		self.waits_on.len() == 0
	}

	fn valid(&self) -> bool {
		self.ready() && self.depends_on.len() == 0
	}

	fn raw(&self) -> &IndexedBlock {
		&self.block
	}

	fn waits_mut(&mut self) -> &mut HashSet<H256> {
		&mut self.waits_on
	}

	fn depends_mut(&mut self) -> &mut HashSet<H256> {
		&mut self.depends_on
	}
}

pub trait DependencyDenote {
	fn denote_transaction(&self, hash: &H256);
}

impl<T> DependencyDenote for T where T: TransactionProvider + TransactionMetaProvider {
	fn denote_transaction(&self, hash: &H256)  {
		// this should put transacion and meta in the lru cache
		self.transaction(hash);
		self.transaction_meta(hash);
	}
}

// locks are aquired in the order in the struct
pub struct BlockQueue {
	// location of transactions which are still in queued blocks, with the block hash locator
	pending_transactions: RwLock<HashMap<H256, H256>>,
	// location of transactions which are in verified blocks, with the block hash locator
	verified_transactions: RwLock<HashMap<H256, H256>>,
	// pushed blocks
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

pub enum TaskResult { Ok, Wait }

pub struct BlockQueueSummary {
	added: usize,
	unverified: usize,
	verified: usize,
}

pub trait VerifyBlock {
	type Error;

	fn verify(&self, block: &IndexedBlock) -> Result<(), Self::Error>;
}

impl BlockQueue {
	// new queue

	pub fn new() -> BlockQueue {
		BlockQueue {
			pending_transactions: Default::default(),
			verified_transactions: Default::default(),
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
	pub fn fetch(&self, dependencies: &DependencyDenote) -> TaskResult {
		let mut block: PreparedBlock = {
			let mut added_lock = self.added.write();
			let mut processing_lock = self.processing.write();
			let (hash, block) = match added_lock.pop_front() {
				None => { return TaskResult::Wait },
				Some(b) => b,
			};
			processing_lock.insert(hash);
			block
		}.into();

		// many of the cases won't use it, so by default it does not allocate anything
		let mut waits = HashSet::with_capacity(0);
		let mut depends = HashSet::with_capacity(0);

		for block_tx in block.raw().transactions.iter() {
			for input in block_tx.raw.inputs.iter().skip(1) {
				let parent_tx_hash = &input.previous_output.hash;

				if let Some(block_locator) = self.pending_transactions.read().get(parent_tx_hash) {
					waits.insert(block_locator.clone());
				} else if let Some(block_locator) = self.verified_transactions.read().get(parent_tx_hash) {
					depends.insert(block_locator.clone());
				} else {
					// todo: maybe use local cache
					// todo: maybe scan block first (depends on how often transactions reference parents in the same block)
					dependencies.denote_transaction(parent_tx_hash);
				}
			}
		}

		*block.waits_mut() = waits;
		*block.depends_mut() = depends;

		{
			let mut unverified = self.unverified.write();
			let mut processing = self.processing.write();
			let mut pending_txes = self.pending_transactions.write();

			for tx in block.raw().transactions.iter() {
				pending_txes.insert(tx.hash.clone(), block.raw().hash().clone());
			}

			processing.remove(block.raw().hash());
			unverified.insert(block.raw().hash().clone(), block);
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
				// check if block dependencies are fetched and if it has
				// verified or known parent
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
			Ok(_) => {
				// todo: chain notify
				{
					let verified_hash = block.raw().hash().clone();

					let mut pending_txes = self.pending_transactions.write();
					let mut verified_txes = self.verified_transactions.write();
					let mut unverified = self.unverified.write();
					let mut processing = self.processing.write();
					let mut verified = self.verified.write();

					// kick pending blocks to proceed
					for (_, mut block) in unverified.iter_mut() {
						if block.waits_mut().remove(&verified_hash) {
							block.depends_mut().insert(verified_hash.clone());
						}
					}

					// move transactions from pending to verified
					for tx in block.raw().transactions.iter() {
						if let Entry::Occupied(entry) = pending_txes.entry(tx.hash.clone()) {
							let (key, val) = entry.remove_entry();
							verified_txes.insert(key, val);
						}
					}
					processing.remove(&verified_hash);

					verified.insert(verified_hash, block);
				}
			},
			Err(_) => {
				// todo: chain notify

				let mut pending_txes = self.pending_transactions.write();
				let mut processing = self.processing.write();
				let mut invalid = self.invalid.write();

				// remove transactions from pending
				for tx in block.raw().transactions.iter() {
					pending_txes.remove(&tx.hash);
				}

				processing.remove(block.raw().hash());

				invalid.insert(block.raw().hash().clone());
			}
		}

		TaskResult::Ok
	}
}

#[cfg(test)]
mod tests {

	use super::{BlockQueue, VerifyBlock};
	use storage::Storage;
	use devtools::RandomTempPath;
	use test_data;
	use chain::IndexedBlock;
	use block_stapler::BlockStapler;

	struct DummyError;
	struct FacileVerifier;

	impl VerifyBlock for FacileVerifier {
		type Error = DummyError;

		fn verify(&self, _block: &IndexedBlock) -> Result<(), DummyError> {
			Ok(())
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

		let b1: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().build()
				.build()
			.merkled_header().parent(genesis_hash).build()
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

	#[test]
	fn fetch_tx_overlay() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_hash = genesis.hash();
		let genesis_coinbase = genesis.transactions()[0].hash();


		let b1: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().build()
				.build()
			.merkled_header().parent(genesis_hash).build()
			.build()
			.into();

		let queue = BlockQueue::new();
		queue.push(b1);
		queue.fetch(&store);

		// coinbase + one regular transaction
		assert_eq!(queue.pending_transactions.read().len(), 2);
	}


	#[test]
	fn dependant_block() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_hash = genesis.hash();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let b1: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().build()
				.build()
			.merkled_header().parent(genesis_hash).build()
			.build()
			.into();
		let unspent_tx = b1.transactions[1].hash.clone();
		let test_hash_original = b1.hash().clone();

		let b2: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(5).build()
				.build()
			.transaction()
				.input().hash(unspent_tx).build()
				.output().build()
				.build()
			.merkled_header().parent(b1.hash().clone()).build()
			.build()
			.into();
		let test_hash = b2.hash().clone();

		let queue = BlockQueue::new();
		queue.push(b1);
		queue.push(b2);
		queue.fetch(&store);
		queue.fetch(&store);

		assert!(
			queue.unverified.read().get(&test_hash)
				.expect("There should be b2 in unverified list")
				.waits_on
				.contains(&test_hash_original)
		);
	}

}
