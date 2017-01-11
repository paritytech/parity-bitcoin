use primitives::hash::H256;
use parking_lot::{RwLock, Mutex};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::sync::{Arc, Weak};

use linked_hash_map::LinkedHashMap;
use chain::{IndexedBlock, IndexedTransaction};
use transaction_provider::TransactionProvider;
use transaction_meta_provider::TransactionMetaProvider;
use block_stapler::{BlockStapler, BlockInsertedChain};
use error::{Error as StorageError, VerificationError, ConsistencyError};

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
	// verified blocks
	verified: RwLock<LinkedHashMap<H256, PreparedBlock>>,
	// on verification
	verifying: RwLock<HashSet<H256>>,
	// on fetching
	fetching: RwLock<HashSet<H256>>,
	// invalid blocks
	invalid: RwLock<HashSet<H256>>,
	// notifiers
	notify: Mutex<Vec<QueueNotifyEntry>>,
}

#[derive(Debug)]
pub enum TaskResult { Ok, Wait }

pub struct BlockQueueSummary {
	pub added: usize,
	pub unverified: usize,
	pub verified: usize,
	pub invalid: usize,
	pub processing: usize,
}

pub trait VerifyBlock {
	fn verify(&self, block: &IndexedBlock) -> Result<(), VerificationError>;
}

pub trait InsertBlock {
	fn insert(&self, block: &IndexedBlock) -> Result<BlockInsertedChain, ConsistencyError>;
}

impl<T> InsertBlock for T where T: BlockStapler {
	fn insert(&self, block: &IndexedBlock) -> Result<BlockInsertedChain, ConsistencyError> {
		match self.insert_indexed_block(block) {
			Ok(route) => Ok(route),
			Err(StorageError::Consistency(c_err)) => Err(c_err),
			Err(e) => {
				// unknown error (io/corruption) should fail fast
				panic!("Unexpected error on block insertion: {:?}", e);
			}
		}
	}
}

pub trait QueueNotify : Send + Sync {
	fn queue_verified(&self, _block: &IndexedBlock) {
	}

	fn queue_block(&self, _block: &IndexedBlock, _route: BlockInsertedChain) {
	}

	fn queue_transaction(&self, _transaction: &IndexedTransaction) {
	}
}

pub struct QueueNotifyEntry {
	subscriber: Weak<QueueNotify>,
	notify_verified: bool,
	notify_block: bool,
	notify_transaction: bool,
}

impl QueueNotifyEntry {
	pub fn new(subscriber: &Arc<QueueNotify>) -> Self {
		QueueNotifyEntry {
			subscriber: Arc::downgrade(subscriber),
			notify_verified: false,
			notify_block: false,
			notify_transaction: false,
		}
	}

	pub fn verified(mut self, notify: bool) -> Self {
		self.notify_verified = notify;
		self
	}

	pub fn block(mut self, notify: bool) -> Self {
		self.notify_block = notify;
		self
	}

	pub fn transaction(mut self, notify: bool) -> Self {
		self.notify_transaction = notify;
		self
	}
}

impl BlockQueue {
	// new queue

	pub fn new() -> BlockQueue {
		BlockQueue {
			pending_transactions: Default::default(),
			verified_transactions: Default::default(),
			added: Default::default(),
			unverified: Default::default(),
			verified: Default::default(),
			invalid: Default::default(),
			notify: Default::default(),
			verifying: Default::default(),
			fetching: Default::default(),
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
			invalid: self.invalid.read().len(),
			processing: self.verifying.read().len() + self.fetching.read().len(),
		}
	}

	// converts added indexed block into block prepared for verification
	pub fn fetch(&self, dependencies: &DependencyDenote) -> TaskResult {
		let mut block: PreparedBlock = {
			let mut added = self.added.write();
			let mut fetching = self.fetching.write();
			let (hash, block) = match added.pop_front() {
				None => { return TaskResult::Wait },
				Some(b) => b,
			};
			fetching.insert(hash);
			block
		}.into();

		// many of the cases won't use it
		// hence no allocation by default
		let mut waits = HashSet::with_capacity(0);
		let mut depends = HashSet::with_capacity(0);

		trace!(target: "db", "fetching block hash {}, tx count {}", block.raw().hash(), block.raw().transactions.len());
		for block_tx in block.raw().transactions.iter().skip(1) {
			for input in block_tx.raw.inputs.iter() {
				let parent_tx_hash = &input.previous_output.hash;
				trace!(target: "db", "fetching parent transaction {}", parent_tx_hash);

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
			let mut pending_txes = self.pending_transactions.write();
			let mut unverified = self.unverified.write();
			let mut fetching = self.fetching.write();

			for tx in block.raw().transactions.iter() {
				pending_txes.insert(tx.hash.clone(), block.raw().hash().clone());
			}

			fetching.remove(block.raw().hash());
			unverified.insert(block.raw().hash().clone(), block);
		}

		TaskResult::Ok
	}

	pub fn verify(&self, verifier: &VerifyBlock) -> TaskResult {
		// will return first block that is ready to verify
		let block = {
			let mut unverified = self.unverified.write();
			let mut verifying = self.verifying.write();
			let fetching = self.fetching.read();

			let mut ready_hash: Option<H256> = None;
			for (hash, block) in unverified.iter() {
				// see if we have a candidate for verification
				// it should on unverfied(fetched) list
				// it's dependencies should be verified
				// and parent should not be in the processing/unverified list
				// (that ensures that no block will go to verification until its
				//  parent does)
				if block.ready() &&
					!fetching.contains(&block.raw().header.raw.previous_header_hash) &&
					!unverified.contains_key(&block.raw().header.raw.previous_header_hash)
				{
					ready_hash = Some(hash.clone());
				}
			}

			match ready_hash {
				None => { return TaskResult::Wait; }
				Some(hash) => {
					let block = unverified.remove(&hash).expect("We just located it above with lock is still on");
					verifying.insert(hash);
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
					let mut verified = self.verified.write();
					let mut verifying = self.verifying.write();

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
					verifying.remove(&verified_hash);

					verified.insert(verified_hash, block);
				}
			},
			Err(_) => {
				// todo: chain notify

				let mut pending_txes = self.pending_transactions.write();
				let mut verifying = self.verifying.write();
				let mut invalid = self.invalid.write();

				// remove transactions from pending
				for tx in block.raw().transactions.iter() {
					pending_txes.remove(&tx.hash);
				}

				verifying.remove(block.raw().hash());

				invalid.insert(block.raw().hash().clone());
			}
		}

		TaskResult::Ok
	}

	/// This is supposed to be quick and synchronous
	/// Since all dependencies are fetched (and flushes are in another thread)
	pub fn insert_verified<I>(&self, inserter: &I) -> TaskResult where I: InsertBlock {
		let mut verified_txes = self.verified_transactions.write();
		let mut verified = self.verified.write();

		// check head block if it is exists and ready for insertion
		match verified.front() {
			Some((_, block)) if block.valid() => { }
			_ => { return TaskResult::Wait; }
		};
		let (hash, block) = verified.pop_front().expect("We just checked above that front item exists; So pop should produce existing item; qed");

		// remove transactions from verified list
		for tx in block.raw().transactions.iter() {
			verified_txes.remove(&tx.hash);
		}

		match inserter.insert(&block.raw()) {
			Ok(route) => {
				// notify about block and transactions (both only if any subscribers)
				for subscriber in self.notify_filtered(|e| e.notify_block) {
					subscriber.queue_block(&block.raw(), route.clone());
				}

				for subscriber in self.notify_filtered(|e| e.notify_transaction) {
					for tx in block.raw().transactions.iter() {
						subscriber.queue_transaction(tx);
					}
				}
			}
			Err(_) => {
				// todo: chain notify
				let mut invalid = self.invalid.write();
				invalid.insert(hash);
				// todo: also invalidate all dependant blocks

			}
		};

		TaskResult::Ok
	}

	pub fn has_invalid(&self, hash: &H256) -> bool {
		self.invalid.read().contains(hash)
	}

	pub fn subscribe(&self, entry: QueueNotifyEntry) {
		self.notify.lock().push(entry);
	}

	fn notify_filtered<F>(&self, f: F) -> Vec<Arc<QueueNotify>>
		where F: Fn(&QueueNotifyEntry) -> bool
	{
		self.notify.lock().iter()
			.filter(|entry| f(entry))
			.filter_map(|entry| entry.subscriber.upgrade())
			.collect()
	}
}

#[cfg(test)]
mod tests {

	use std::sync::Arc;
	use parking_lot::RwLock;
	use super::{BlockQueue, VerifyBlock, InsertBlock, QueueNotifyEntry, QueueNotify};
	use storage::Storage;
	use devtools::RandomTempPath;
	use test_data;
	use chain::IndexedBlock;
	use block_stapler::{BlockStapler, BlockInsertedChain};
	use error::{VerificationError, ConsistencyError};

	struct FacileVerifier;

	impl VerifyBlock for FacileVerifier {
		fn verify(&self, _block: &IndexedBlock) -> Result<(), VerificationError> {
			Ok(())
		}
	}

	struct FacileInserter;

	impl InsertBlock for FacileInserter {
		fn insert(&self, _block: &IndexedBlock) -> Result<BlockInsertedChain, ConsistencyError> {
			Ok(BlockInsertedChain::Main)
		}
	}

	#[test]
	fn push() {
		let queue = BlockQueue::new();

		queue.push(test_data::block_h2().into());

		assert_eq!(queue.summary().added, 1);
	}

	/// Fetching moves the block from `added` line to `unverified`,
	#[test]
	fn fetch() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();
		let queue = BlockQueue::new();
		queue.push(test_data::block_h2().into());

		queue.fetch(&store);

		assert_eq!(queue.summary().unverified, 1);
		assert_eq!(queue.summary().added, 0);
		assert_eq!(queue.summary().invalid, 0);
		assert_eq!(queue.summary().verified, 0);

	}

	/// Verification moves the block from `unverified` line to `verified`
	/// if provided verifier returns Ok(_)
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
		assert_eq!(queue.summary().invalid, 0);
	}


	#[test]
	fn insert() {
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

		queue.insert_verified(&FacileInserter);

		assert_eq!(queue.summary().verified, 0);
		assert_eq!(queue.summary().unverified, 0);
		assert_eq!(queue.summary().added, 0);
		assert_eq!(queue.summary().invalid, 0);
	}

	/// Once block is fetched, all it transactions should be put in `pending_transactions`
	/// So that next blocks are aware that upstream blocks may contain transaction they spend
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

	/// Once block is verified, all it transactions should be moved from `pending_transactions`
	/// to `verified_transactions`, so that next blocks are aware that upstream blocks may contain
	/// transaction they spend and they are verified
	#[test]
	fn verify_tx_overlay() {
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

		// 2 transactions of b1
		assert_eq!(queue.verified_transactions.read().len(), 2);
	}

	/// If fetching the block which reference transaction in the block not yet processed,
	/// it is marked as waiting on that latter block
	/// It will not get started to verify until master block is made it to the `verified`
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

	/// Once block, that is current block dependant on, verified,
	/// current block is no longer waits for it
	/// but still depends on it since it will become invalid on insertion, the current block
	/// is also will become invalid
	#[test]
	fn dependant_block_verified() {
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
		queue.verify(&FacileVerifier);

		assert!(
			queue.unverified.read().get(&test_hash)
				.expect("There should be b2 in unverified list")
				.depends_on
				.contains(&test_hash_original)
		);
	}

	#[test]
	fn notification() {

		#[derive(Default)]
		struct NotificationRecorder {
			count: RwLock<u32>,
		}

		impl QueueNotify for NotificationRecorder {
			fn queue_block(&self, _block: &IndexedBlock, _route: BlockInsertedChain) {
				*self.count.write() += 1
			}
		}

		let counter = Arc::new(NotificationRecorder::default());
		let subscriber = counter.clone() as Arc<QueueNotify>;

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();
		let queue = BlockQueue::new();
		queue.subscribe(QueueNotifyEntry::new(&subscriber).block(true));

		queue.push(test_data::block_h2().into());
		queue.fetch(&store);
		queue.verify(&FacileVerifier);
		queue.insert_verified(&FacileInserter);

		let count = *counter.count.read();
		assert_eq!(1, count);
	}
}
