use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::path::Path;
use std::thread;
use parking_lot::Mutex;

use storage::{Store, Storage};
use block_stapler::{BlockInsertedChain, BlockStapler};
use block_queue::{BlockQueue, VerifyBlock, TaskResult, QueueNotify};
use chain::{IndexedBlock, IndexedTransaction, Block};
use error::Error as StorageError;

const MAX_BLOCK_QUEUE: usize = 256;

const FETCH_THREADS: usize = 4;
// transaction parallelisation allows to use only 1 verification threads
const VERIFICATION_THREADS: usize = 1;
const INSERT_THREADS: usize = 1;
const FLUSH_THREADS: usize = 2;

const FLUSH_INTERVAL: u64 = 200;
const TASK_TIMEOUT_INTERVAL: u64 = 100;

pub trait ChainNotify : Send + Sync {
	fn chain_block(&self, _block: &IndexedBlock, _route: BlockInsertedChain) {
	}

	fn chain_transaction(&self, _transaction: &IndexedTransaction) {
	}
}

pub struct ChainNotifyEntry {
	subscriber: Weak<ChainNotify>,
	notify_blocks: bool,
	notify_transactions: bool,
}

impl ChainNotifyEntry {
	pub fn new(subscriber: Arc<ChainNotify>) -> ChainNotifyEntry {
		ChainNotifyEntry {
			subscriber: Arc::downgrade(&subscriber),
			notify_blocks: false,
			notify_transactions: false,
		}
	}

	pub fn blocks(mut self, notify: bool) -> Self {
		self.notify_blocks = notify;
		self
	}

	pub fn transactions(mut self, notify: bool) -> Self {
		self.notify_transactions = notify;
		self
	}
}

pub struct ChainClient {
	store: Arc<Storage>,
	queue: Arc<BlockQueue>,
	fetch_threads: Vec<JoinHandle<()>>,
	verification_threads: Vec<JoinHandle<()>>,
	insert_threads: Vec<JoinHandle<()>>,
	flush_threads: Vec<JoinHandle<()>>,
	notify: Mutex<Vec<ChainNotifyEntry>>,
	stop: Arc<AtomicBool>,
	verifier_factory: Box<VerifierFactory>,
}

pub trait VerifierFactory : Send + Sync {
	fn spawn(&self, db: Arc<Store>) -> Box<VerifyBlock + Send>;

	fn genesis(&self) -> Option<Block>;
}

#[derive(Debug)]
pub enum PushBlockError {
	QueueFull,
	ParentInvalid,
}

impl ChainClient {

	pub fn new<P: AsRef<Path>>(path: P, verifier_factory: Box<VerifierFactory>) -> Result<ChainClient, StorageError> {
		let store = Arc::new(try!(Storage::new(path)));
		let mut chain = ChainClient {
			store: store,
			queue: Arc::new(BlockQueue::new()),
			fetch_threads: Vec::new(),
			verification_threads: Vec::new(),
			insert_threads: Vec::new(),
			flush_threads: Vec::new(),
			notify: Default::default(),
			stop: Arc::new(AtomicBool::new(false)),
			verifier_factory: verifier_factory,
		};

		if let Some(genesis) = chain.verifier_factory.genesis() {
			try!(chain.store.insert_block(&genesis));
		}

		for _ in 0..FETCH_THREADS {
			let thread = chain.fetch_thread();
			chain.fetch_threads.push(thread);
		}
		for _ in 0..VERIFICATION_THREADS {
			let thread = chain.verification_thread();
			chain.fetch_threads.push(thread);
		}
		for _ in 0..FLUSH_THREADS {
			let thread = chain.flush_thread();
			chain.fetch_threads.push(thread);
		}
		for _ in 0..INSERT_THREADS {
			let thread = chain.insert_thread();
			chain.fetch_threads.push(thread);
		}

		Ok(chain)
	}

	pub fn push_block(&self, block: IndexedBlock) -> Result<(), PushBlockError> {
		{
			let parent_hash = &block.header.raw.previous_header_hash;
			if self.queue().has_invalid(parent_hash) {
				return Err(PushBlockError::ParentInvalid);
			}

			if self.queue().summary().added >= MAX_BLOCK_QUEUE {
				return Err(PushBlockError::QueueFull);
			}
		}

		self.queue().push(block);

		Ok(())
	}

	pub fn subcribe(&self, notify: ChainNotifyEntry) {
		self.notify.lock().push(notify);
	}

	pub fn store(&self) -> &Storage {
		&self.store
	}

	pub fn queue(&self) -> &BlockQueue {
		&self.queue
	}

	fn flush_thread(&self) -> JoinHandle<()> {
		let thread_stop = self.stop.clone();
		let thread_store = self.store.clone();
		thread::spawn(move || {
			while !thread_stop.load(Ordering::SeqCst) {
				thread_store.flush();
				thread::park_timeout(::std::time::Duration::from_millis(FLUSH_INTERVAL));
			}
		})
	}

	fn verification_thread(&self) -> JoinHandle<()> {
		let thread_stop = self.stop.clone();
		let thread_verifier = self.verifier_factory.spawn(self.store.clone());
		let thread_queue = self.queue.clone();

		thread::spawn(move || {
			while !thread_stop.load(Ordering::SeqCst) {
				match thread_queue.verify(&*thread_verifier) {
					TaskResult::Ok => { } // continue with next block
					TaskResult::Wait => {
						thread::park_timeout(::std::time::Duration::from_millis(TASK_TIMEOUT_INTERVAL));
					}
				}
			}
		})
	}

	fn fetch_thread(&self) -> JoinHandle<()> {
		let thread_stop = self.stop.clone();
		let thread_store = self.store.clone();
		let thread_queue = self.queue.clone();

		thread::spawn(move || {
			while !thread_stop.load(Ordering::SeqCst) {
				match thread_queue.fetch(&*thread_store) {
					TaskResult::Ok => { } // continue with next block
					TaskResult::Wait => {
						thread::park_timeout(::std::time::Duration::from_millis(TASK_TIMEOUT_INTERVAL));
					}
				}
			}
		})
	}

	fn insert_thread(&self) -> JoinHandle<()> {
		let thread_stop = self.stop.clone();
		let thread_store = self.store.clone();
		let thread_queue = self.queue.clone();

		thread::spawn(move || {
			while !thread_stop.load(Ordering::SeqCst) {
				match thread_queue.insert_verified(&*thread_store) {
					TaskResult::Ok => { } // continue with next block
					TaskResult::Wait => {
						thread::park_timeout(::std::time::Duration::from_millis(TASK_TIMEOUT_INTERVAL));
					}
				}
			}
		})
	}

	pub fn flush(&self) {
		while {
			let summary = self.queue.summary();
			summary.added != 0 || summary.verified != 0 || summary.unverified != 0 || summary.processing != 0
		} {
			thread::park_timeout(::std::time::Duration::from_millis(TASK_TIMEOUT_INTERVAL));
		}
	}

	fn notify_filtered<F>(&self, f: F) -> Vec<Arc<ChainNotify>>
		where F: Fn(&ChainNotifyEntry) -> bool
	{
		self.notify.lock().iter()
			.filter(|entry| f(entry))
			.filter_map(|entry| entry.subscriber.upgrade())
			.collect()
	}
}

impl QueueNotify for ChainClient {
	fn queue_block(&self, block: &IndexedBlock, route: BlockInsertedChain) {
		for subscriber in self.notify_filtered(|e| e.notify_blocks) {
			subscriber.chain_block(block, route.clone());
		}
	}
}

impl Drop for ChainClient {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::SeqCst);
		for thread in self.insert_threads.drain(..) { thread.join().expect("Failed to join insert thread"); }
		for thread in self.flush_threads.drain(..) { thread.join().expect("Failed to join flush thread"); }
		for thread in self.verification_threads.drain(..) { thread.join().expect("Failed to join verification thread"); }
		for thread in self.fetch_threads.drain(..) { thread.join().expect("Failed to join fetch thread"); }
		self.store.flush();
	}
}

#[cfg(test)]
mod tests {

	use super::{ChainClient, VerifierFactory};
	use block_queue::VerifyBlock;
	use devtools::RandomTempPath;
	use chain::{IndexedBlock, Block};
	use error::VerificationError;
	use std::sync::Arc;
	use storage::Store;
	use test_data;

	struct FacileVerifier;
	struct FacileFactory;

	impl VerifierFactory for FacileFactory {
		fn spawn(&self, _db: Arc<Store>) -> Box<VerifyBlock + Send> {
			Box::new(FacileVerifier)
		}

		fn genesis(&self) -> Option<Block> {
			Some(test_data::genesis())
		}
	}

	impl VerifyBlock for FacileVerifier {
		fn verify(&self, _block: &IndexedBlock) -> Result<(), VerificationError> {
			Ok(())
		}

	}

	#[test]
	fn smoky() {
		let path = RandomTempPath::create_dir();
		let client = ChainClient::new(path.as_path(), Box::new(FacileFactory)).expect("Client should be created");

		client.push_block(test_data::block_h1().into()).expect("block height 1 should be inserted");
		client.flush();

		assert_eq!(
			client.store().best_block().expect("There should be best block").hash,
			test_data::block_h1().hash()
		);
	}
}
