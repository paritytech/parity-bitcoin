use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread;
use parking_lot::Mutex;
use time::get_time;
use chain::{IndexedBlock, IndexedTransaction};
use db::{SharedStore, PreviousTransactionOutputProvider, TransactionOutputObserver};
use network::Magic;
use primitives::hash::H256;
use verification::{BackwardsCompatibleChainVerifier as ChainVerifier, Verify as VerificationVerify, Chain};
use types::{StorageRef, MemoryPoolRef};
use utils::{MemoryPoolTransactionOutputProvider, StorageTransactionOutputProvider};

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
	VerifyTransaction(u32, IndexedTransaction),
	/// Stop verification thread
	Stop,
}

/// Synchronization verifier
pub trait Verifier : Send + Sync + 'static {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock);
	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: IndexedTransaction);
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
		match self {
			&VerificationTask::VerifyTransaction(_, ref transaction) => Some(&transaction),
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
				.expect("Error creating verification thread"))
		}
	}

	/// Thread procedure for handling verification tasks
	fn verification_worker_proc<T: VerificationSink>(sink: Arc<T>, storage: StorageRef, memory_pool: MemoryPoolRef, verifier: Arc<ChainVerifier>, work_receiver: Receiver<VerificationTask>) {
		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::Stop => break,
				_ => {
					let prevout_provider = if let Some(ref transaction) = task.transaction() {
						match MemoryPoolTransactionOutputProvider::for_transaction(storage.clone(), &memory_pool, &transaction.raw) {
							Err(e) => {
								sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash);
								return;
							},
							Ok(prevout_provider) => prevout_provider,
						}
					} else {
						MemoryPoolTransactionOutputProvider::for_block(storage.clone())
					};
					execute_verification_task(&sink, &prevout_provider, &verifier, task)
				},
			}
		}
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
	fn verify_transaction(&self, height: u32, transaction: IndexedTransaction) {
		self.verification_work_sender.lock()
			.send(VerificationTask::VerifyTransaction(height, transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

/// Synchronous synchronization verifier
pub struct SyncVerifier<T: VerificationSink> {
	/// Storage reference
	storage: StorageRef,
	/// Verifier
	verifier: ChainVerifier,
	/// Verification sink
	sink: Arc<T>,
}

impl<T> SyncVerifier<T> where T: VerificationSink {
	/// Create new sync verifier
	pub fn new(network: Magic, storage: SharedStore, sink: Arc<T>) -> Self {
		let verifier = ChainVerifier::new(storage.clone(), network);
		SyncVerifier {
			storage: storage,
			verifier: verifier,
			sink: sink,
		}
}
	}

impl<T> Verifier for SyncVerifier<T> where T: VerificationSink {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock) {
		let prevout_provider = StorageTransactionOutputProvider::with_storage(self.storage.clone());
		execute_verification_task(&self.sink, &prevout_provider, &self.verifier, VerificationTask::VerifyBlock(block))
	}

	/// Verify transaction
	fn verify_transaction(&self, _height: u32, _transaction: IndexedTransaction) {
		unimplemented!() // sync verifier is currently only used for blocks verification
	}
}

/// Execute single verification task
fn execute_verification_task<T: VerificationSink, U: TransactionOutputObserver + PreviousTransactionOutputProvider>(sink: &Arc<T>, tx_output_provider: &U, verifier: &ChainVerifier, task: VerificationTask) {
	let mut tasks_queue: VecDeque<VerificationTask> = VecDeque::new();
	tasks_queue.push_back(task);

	while let Some(task) = tasks_queue.pop_front() {
		// TODO: for each task different output provider must be created
		// reorg => txes from storage are reverifying + mempool txes are reverifying
		// => some mempool can be invalid after reverifying
		match task {
			VerificationTask::VerifyBlock(block) => {
				// verify block
				match verifier.verify(&block) {
					Ok(Chain::Main) | Ok(Chain::Side) => {
						if let Some(tasks) = sink.on_block_verification_success(block) {
							tasks_queue.extend(tasks);
						}
					},
					Ok(Chain::Orphan) => {
						// this can happen for B1 if B0 verification has failed && we have already scheduled verification of B0
						sink.on_block_verification_error(&format!("orphaned block because parent block verification has failed"), &block.hash())
					},
					Err(e) => {
						sink.on_block_verification_error(&format!("{:?}", e), &block.hash())
					}
				}
			},
			VerificationTask::VerifyTransaction(height, transaction) => {
				let time: u32 = get_time().sec as u32;
				match verifier.verify_mempool_transaction(tx_output_provider, height, time, &transaction.raw) {
					Ok(_) => sink.on_transaction_verification_success(transaction.into()),
					Err(e) => sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash),
				}
			},
			_ => unreachable!("must be checked by caller"),
		}
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::HashMap;
	use synchronization_client_core::CoreVerificationSink;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use chain::{IndexedBlock, IndexedTransaction};
	use super::{Verifier, BlockVerificationSink, TransactionVerificationSink};

	#[derive(Default)]
	pub struct DummyVerifier {
		sink: Option<Arc<CoreVerificationSink<DummyTaskExecutor>>>,
		errors: HashMap<H256, String>
	}

	impl DummyVerifier {
		pub fn set_sink(&mut self, sink: Arc<CoreVerificationSink<DummyTaskExecutor>>) {
			self.sink = Some(sink);
		}

		pub fn error_when_verifying(&mut self, hash: H256, err: &str) {
			self.errors.insert(hash, err.into());
		}
	}

	impl Verifier for DummyVerifier {
		fn verify_block(&self, block: IndexedBlock) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&block.hash()) {
					Some(err) => sink.on_block_verification_error(&err, &block.hash()),
					None => {
						sink.on_block_verification_success(block);
						()
					},
				},
				None => panic!("call set_sink"),
			}
		}

		fn verify_transaction(&self, _height: u32, transaction: IndexedTransaction) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&transaction.hash) {
					Some(err) => sink.on_transaction_verification_error(&err, &transaction.hash),
					None => sink.on_transaction_verification_success(transaction.into()),
				},
				None => panic!("call set_sink"),
			}
		}
	}
}
