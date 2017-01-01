use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::thread::JoinHandle;
use std::path::Path;
use std::thread;
use parking_lot::Mutex;

use storage::{Store, Storage};
use block_stapler::{BlockInsertedChain, BlockStapler};
use block_queue::{BlockQueue, VerifyBlock, TaskResult};
use chain::{IndexedBlock, IndexedTransaction};
use error::{VerificationError, Error as StorageError};

const MAX_BLOCK_QUEUE: usize = 256;

const FETCH_THREADS: usize = 4;
// transaction parallelisation allows to use only 1 verification threads
const VERIFICATION_THREADS: usize = 1;
const INSERT_THREADS: usize = 1;
const FLUSH_THREADS: usize = 1;

const FLUSH_INTERVAL: u64 = 200;
const TASK_TIMEOUT_INTERVAL: u64 = 100;

trait ChainNotify {
	fn block(&self, block: &IndexedBlock, route: BlockInsertedChain) {
	}

	fn transaction(&self, transaction: &IndexedTransaction) {
	}
}

struct ChainNotifyEntry {
	subscriber: Weak<ChainNotify>,
	notify_blocks: bool,
	notify_transactions: bool,
}

impl ChainNotifyEntry {
}

struct ChainClient {
	store: Arc<Storage>,
	verifier: Arc<VerifyBlock>,
	queue: Arc<BlockQueue>,
	fetch_threads: Vec<JoinHandle<()>>,
	verification_threads: Vec<JoinHandle<()>>,
	insert_threads: Vec<JoinHandle<()>>,
	flush_threads: Vec<JoinHandle<()>>,
	notify: Mutex<Vec<ChainNotifyEntry>>,
	stop: Arc<AtomicBool>,
}

pub trait VerifierFactory {
	fn spawn(&self, db: Arc<Store>) -> Arc<VerifyBlock>;
}

pub enum PushBlockError {
	QueueFull,
	ParentInvalid,
}

impl ChainClient {

	pub fn new<P: AsRef<Path>, V: VerifierFactory>(path: P, verifier_factory: V) -> Result<ChainClient, StorageError> {
		let store = Arc::new(try!(Storage::new(path)));
		let verifier = verifier_factory.spawn(store.clone());

		let mut chain = ChainClient {
			store: store,
			verifier: verifier,
			queue: Arc::new(BlockQueue::new()),
			fetch_threads: Vec::new(),
			verification_threads: Vec::new(),
			insert_threads: Vec::new(),
			flush_threads: Vec::new(),
			notify: Default::default(),
			stop: Arc::new(AtomicBool::new(false)),
		};

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

	fn store(&self) -> &Arc<Storage> {
		&self.store
	}

	fn queue(&self) -> &BlockQueue {
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
		let thread_verifier = self.verifier.clone();
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

}
