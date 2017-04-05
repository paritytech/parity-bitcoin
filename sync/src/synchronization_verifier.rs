use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use parking_lot::Mutex;
use time::get_time;
use chain::{IndexedBlock, IndexedTransaction};
use network::Magic;
use primitives::hash::H256;
use verification::{BackwardsCompatibleChainVerifier as ChainVerifier, Verify as VerificationVerify};
use types::{BlockHeight, StorageRef, MemoryPoolRef};
use utils::MemoryPoolTransactionOutputProvider;

/// Block verification events sink
pub trait BlockVerificationSink : Send + Sync + 'static {
	/// When block verification has completed successfully.
	fn on_block_verification_success(&self, block: IndexedBlock) -> Option<Vec<VerificationTask>>;
	/// When block verification has failed.
	fn on_block_verification_error(&self, err: &str, hash: &H256);
}

/// Transaction verification events sink
pub trait TransactionVerificationSink : Send + Sync + 'static {
	/// When transaction verification has completed successfully.
	fn on_transaction_verification_success(&self, transaction: IndexedTransaction);
	/// When transaction verification has failed.
	fn on_transaction_verification_error(&self, err: &str, hash: &H256);
}

/// Verification events sink
pub trait VerificationSink : BlockVerificationSink + TransactionVerificationSink {
}

/// Verification thread tasks
#[derive(Debug)]
pub enum VerificationTask {
	/// Verify single block
	VerifyBlock(IndexedBlock),
	/// Verify single transaction
	VerifyTransaction(BlockHeight, IndexedTransaction),
	/// Stop verification thread
	Stop,
}

/// Synchronization verifier
pub trait Verifier : Send + Sync + 'static {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock);
	/// Verify transaction
	fn verify_transaction(&self, height: BlockHeight, transaction: IndexedTransaction);
}

/// Asynchronous synchronization verifier
pub struct AsyncVerifier {
	/// Verification work transmission channel.
	verification_work_sender: Mutex<Sender<VerificationTask>>,
	/// Verification thread.
	verification_worker_thread: Option<thread::JoinHandle<()>>,
}

impl VerificationTask {
	/// Returns transaction reference if it is transaction verification task
	pub fn transaction(&self) -> Option<&IndexedTransaction> {
		match *self {
			VerificationTask::VerifyTransaction(_, ref transaction) => Some(transaction),
			_ => None,
		}
	}
}

impl AsyncVerifier {
	/// Create new async verifier
	pub fn new<T: VerificationSink>(verifier: Arc<ChainVerifier>, storage: StorageRef, memory_pool: MemoryPoolRef, sink: Arc<T>) -> Self {
		let (verification_work_sender, verification_work_receiver) = channel();
		AsyncVerifier {
			verification_work_sender: Mutex::new(verification_work_sender),
			verification_worker_thread: Some(thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					AsyncVerifier::verification_worker_proc(sink, storage, memory_pool, verifier, verification_work_receiver)
				})
				.expect("Error creating sync verification thread"))
		}
	}

	/// Thread procedure for handling verification tasks
	fn verification_worker_proc<T: VerificationSink>(sink: Arc<T>, storage: StorageRef, memory_pool: MemoryPoolRef, verifier: Arc<ChainVerifier>, work_receiver: Receiver<VerificationTask>) {
		while let Ok(task) = work_receiver.recv() {
			if !AsyncVerifier::execute_single_task(&sink, &storage, &memory_pool, &verifier, task) {
				break;
			}
		}

		trace!(target: "sync", "Stopping sync verification thread");
	}

	/// Execute single verification task
	pub fn execute_single_task<T: VerificationSink>(sink: &Arc<T>, storage: &StorageRef, memory_pool: &MemoryPoolRef, verifier: &Arc<ChainVerifier>, task: VerificationTask) -> bool {
		// block verification && insertion can lead to reorganization
		// => transactions from decanonized blocks should be put back to the MemoryPool
		// => they must be verified again
		// => here's sub-tasks queue
		let mut tasks_queue: VecDeque<VerificationTask> = VecDeque::new();
		tasks_queue.push_back(task);

		while let Some(task) = tasks_queue.pop_front() {
			match task {
				VerificationTask::VerifyBlock(block) => {
					// verify block
					match verifier.verify(&block) {
						Ok(_) => {
							if let Some(tasks) = sink.on_block_verification_success(block) {
								tasks_queue.extend(tasks);
							}
						},
						Err(e) => {
							sink.on_block_verification_error(&format!("{:?}", e), block.hash())
						}
					}
				},
				VerificationTask::VerifyTransaction(height, transaction) => {
					// output provider must check previous outputs in both storage && memory pool
					match MemoryPoolTransactionOutputProvider::for_transaction(storage.clone(), memory_pool, &transaction.raw) {
						Err(e) => {
							sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash);
							continue; // with new verification sub-task
						},
						Ok(tx_output_provider) => {
							let time: u32 = get_time().sec as u32;
							match verifier.verify_mempool_transaction(&tx_output_provider, height, time, &transaction.raw) {
								Ok(_) => sink.on_transaction_verification_success(transaction.into()),
								Err(e) => sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash),
							}
						},
					};
				},
				VerificationTask::Stop => return false,
			}
		}

		true
	}
}


impl Drop for AsyncVerifier {
	fn drop(&mut self) {
		if let Some(join_handle) = self.verification_worker_thread.take() {
			{
				let verification_work_sender = self.verification_work_sender.lock();
				// ignore send error here <= destructing anyway
				let _ = verification_work_sender.send(VerificationTask::Stop);
			}
			join_handle.join().expect("Clean shutdown.");
		}
	}
}

impl Verifier for AsyncVerifier {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock) {
		self.verification_work_sender.lock()
			.send(VerificationTask::VerifyBlock(block))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}

	/// Verify transaction
	fn verify_transaction(&self, height: BlockHeight, transaction: IndexedTransaction) {
		self.verification_work_sender.lock()
			.send(VerificationTask::VerifyTransaction(height, transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

/// Synchronous synchronization verifier
pub struct SyncVerifier<T: VerificationSink> {
	/// Verifier
	verifier: ChainVerifier,
	/// Verification sink
	sink: Arc<T>,
}

impl<T> SyncVerifier<T> where T: VerificationSink {
	/// Create new sync verifier
	pub fn new(network: Magic, storage: StorageRef, sink: Arc<T>) -> Self {
		let verifier = ChainVerifier::new(storage.clone(), network);
		SyncVerifier {
			verifier: verifier,
			sink: sink,
		}
}
	}

impl<T> Verifier for SyncVerifier<T> where T: VerificationSink {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock) {
		match self.verifier.verify(&block) {
			Ok(_) => {
				// SyncVerifier is used for bulk blocks import only
				// => there are no memory pool
				// => we could ignore decanonized transactions
				self.sink.on_block_verification_success(block);
			},
			Err(e) => self.sink.on_block_verification_error(&format!("{:?}", e), block.hash()),
		}
	}

	/// Verify transaction
	fn verify_transaction(&self, _height: BlockHeight, _transaction: IndexedTransaction) {
		unimplemented!() // sync verifier is currently only used for blocks verification
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::{HashSet, HashMap};
	use verification::BackwardsCompatibleChainVerifier as ChainVerifier;
	use synchronization_client_core::CoreVerificationSink;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use chain::{IndexedBlock, IndexedTransaction};
	use super::{Verifier, BlockVerificationSink, TransactionVerificationSink, AsyncVerifier, VerificationTask};
	use types::{BlockHeight, StorageRef, MemoryPoolRef};

	#[derive(Default)]
	pub struct DummyVerifier {
		sink: Option<Arc<CoreVerificationSink<DummyTaskExecutor>>>,
		errors: HashMap<H256, String>,
		actual_checks: HashSet<H256>,
		storage: Option<StorageRef>,
		memory_pool: Option<MemoryPoolRef>,
		verifier: Option<Arc<ChainVerifier>>,
	}

	impl DummyVerifier {
		pub fn set_sink(&mut self, sink: Arc<CoreVerificationSink<DummyTaskExecutor>>) {
			self.sink = Some(sink);
		}

		pub fn set_storage(&mut self, storage: StorageRef) {
			self.storage = Some(storage);
		}

		pub fn set_memory_pool(&mut self, memory_pool: MemoryPoolRef) {
			self.memory_pool = Some(memory_pool);
		}

		pub fn set_verifier(&mut self, verifier: Arc<ChainVerifier>) {
			self.verifier = Some(verifier);
		}

		pub fn error_when_verifying(&mut self, hash: H256, err: &str) {
			self.errors.insert(hash, err.into());
		}

		pub fn actual_check_when_verifying(&mut self, hash: H256) {
			self.actual_checks.insert(hash);
		}
	}

	impl Verifier for DummyVerifier {
		fn verify_block(&self, block: IndexedBlock) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&block.hash()) {
					Some(err) => sink.on_block_verification_error(&err, &block.hash()),
					None => {
						if self.actual_checks.contains(block.hash()) {
							AsyncVerifier::execute_single_task(sink, self.storage.as_ref().unwrap(), self.memory_pool.as_ref().unwrap(), self.verifier.as_ref().unwrap(), VerificationTask::VerifyBlock(block));
						} else {
							sink.on_block_verification_success(block);
						}
					},
				},
				None => panic!("call set_sink"),
			}
		}

		fn verify_transaction(&self, _height: BlockHeight, transaction: IndexedTransaction) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&transaction.hash) {
					Some(err) => sink.on_transaction_verification_error(&err, &transaction.hash),
					None => {
						if self.actual_checks.contains(&transaction.hash) {
							let next_block_height = self.storage.as_ref().unwrap().best_block().number + 1;
							AsyncVerifier::execute_single_task(sink, self.storage.as_ref().unwrap(), self.memory_pool.as_ref().unwrap(), self.verifier.as_ref().unwrap(), VerificationTask::VerifyTransaction(next_block_height, transaction));
						} else {
							sink.on_transaction_verification_success(transaction.into());
						}
					},
				},
				None => panic!("call set_sink"),
			}
		}
	}
}
