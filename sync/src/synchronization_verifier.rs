use std::thread;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
use chain::{Transaction, OutPoint, TransactionOutput};
use network::Magic;
use primitives::hash::H256;
use synchronization_chain::ChainRef;
use verification::{ChainVerifier, Verify as VerificationVerify, Chain};
use db::{SharedStore, IndexedBlock, PreviousTransactionOutputProvider};

/// Verification events sink
pub trait VerificationSink : Send + 'static {
	/// When block verification has completed successfully.
	fn on_block_verification_success(&mut self, block: IndexedBlock) -> Option<Vec<VerificationTask>>;
	/// When block verification has failed.
	fn on_block_verification_error(&mut self, err: &str, hash: &H256);
	/// When transaction verification has completed successfully.
	fn on_transaction_verification_success(&mut self, transaction: Transaction);
	/// When transaction verification has failed.
	fn on_transaction_verification_error(&mut self, err: &str, hash: &H256);
}

/// Verification thread tasks
#[derive(Debug)]
pub enum VerificationTask {
	/// Verify single block
	VerifyBlock(IndexedBlock),
	/// Verify single transaction
	VerifyTransaction(u32, Transaction),
	/// Stop verification thread
	Stop,
}

/// Synchronization verifier
pub trait Verifier : Send + 'static {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock);
	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: Transaction);
}

/// Asynchronous synchronization verifier
pub struct AsyncVerifier {
	/// Verification work transmission channel.
	verification_work_sender: Sender<VerificationTask>,
	/// Verification thread.
	verification_worker_thread: Option<thread::JoinHandle<()>>,
}

struct ChainMemoryPoolTransactionOutputProvider {
	chain: ChainRef,
}

#[derive(Default)]
struct EmptyTransactionOutputProvider {
}

impl AsyncVerifier {
	/// Create new async verifier
	pub fn new<T: VerificationSink>(network: Magic, chain: ChainRef, sink: Arc<Mutex<T>>) -> Self {
		let (verification_work_sender, verification_work_receiver) = channel();
		let verifier = ChainVerifier::new(chain.read().storage(), network);
		AsyncVerifier {
			verification_work_sender: verification_work_sender,
			verification_worker_thread: Some(thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					AsyncVerifier::verification_worker_proc(sink, chain, verifier, verification_work_receiver)
				})
				.expect("Error creating verification thread"))
		}
	}

	/// Thread procedure for handling verification tasks
	fn verification_worker_proc<T: VerificationSink>(sink: Arc<Mutex<T>>, chain: ChainRef, verifier: ChainVerifier, work_receiver: Receiver<VerificationTask>) {
		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::Stop => break,
				_ => {
					let prevout_provider = ChainMemoryPoolTransactionOutputProvider::with_chain(chain.clone());
					execute_verification_task(&sink, &prevout_provider, &verifier, task)
				},
			}
		}
	}
}


impl Drop for AsyncVerifier {
	fn drop(&mut self) {
		if let Some(join_handle) = self.verification_worker_thread.take() {
			// ignore send error here <= destructing anyway
			let _ = self.verification_work_sender.send(VerificationTask::Stop);
			join_handle.join().expect("Clean shutdown.");
		}
	}
}

impl Verifier for AsyncVerifier {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock) {
		self.verification_work_sender
			.send(VerificationTask::VerifyBlock(block))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}

	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: Transaction) {
		self.verification_work_sender
			.send(VerificationTask::VerifyTransaction(height, transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

/// Synchronous synchronization verifier
pub struct SyncVerifier<T: VerificationSink> {
	/// Verifier
	verifier: ChainVerifier,
	/// Verification sink
	sink: Arc<Mutex<T>>,
}

impl<T> SyncVerifier<T> where T: VerificationSink {
	/// Create new sync verifier
	pub fn new(network: Magic, storage: SharedStore, sink: Arc<Mutex<T>>) -> Self {
		let verifier = ChainVerifier::new(storage, network);
		SyncVerifier {
			verifier: verifier,
			sink: sink,
		}
	}
}

impl<T> Verifier for SyncVerifier<T> where T: VerificationSink {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock) {
		execute_verification_task(&self.sink, &EmptyTransactionOutputProvider::default(), &self.verifier, VerificationTask::VerifyBlock(block))
	}

	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: Transaction) {
		execute_verification_task(&self.sink, &EmptyTransactionOutputProvider::default(), 																&self.verifier, VerificationTask::VerifyTransaction(height, transaction))
	}
}

/// Execute single verification task
fn execute_verification_task<T: VerificationSink, U: PreviousTransactionOutputProvider>(sink: &Arc<Mutex<T>>, tx_output_provider: &U, verifier: &ChainVerifier, task: VerificationTask) {
	let mut tasks_queue: VecDeque<VerificationTask> = VecDeque::new();
	tasks_queue.push_back(task);

	while let Some(task) = tasks_queue.pop_front() {
		match task {
			VerificationTask::VerifyBlock(block) => {
				// verify block
				match verifier.verify(&block) {
					Ok(Chain::Main) | Ok(Chain::Side) => {
						if let Some(tasks) = sink.lock().on_block_verification_success(block) {
							tasks_queue.extend(tasks);
						}
					},
					Ok(Chain::Orphan) => {
						unreachable!("sync will never put orphaned blocks to verification queue");
					},
					Err(e) => {
						sink.lock().on_block_verification_error(&format!("{:?}", e), &block.hash())
					}
				}
			},
			VerificationTask::VerifyTransaction(height, transaction) => {
				// bitcoin: AcceptToMemoryPoolWorker

				let time: u32 = 0; // TODO
				let sequence: usize = 1; // TODO: change to bool
				match verifier.verify_transaction(tx_output_provider, height, time, &transaction, sequence) {
					Ok(_) => sink.lock().on_transaction_verification_success(transaction),
					Err(e) => sink.lock().on_transaction_verification_error(&format!("{:?}", e), &transaction.hash()),
				}
			},
			_ => unreachable!("must be checked by caller"),
		}
	}
}

impl ChainMemoryPoolTransactionOutputProvider {
	pub fn with_chain(chain: ChainRef) -> Self {
		ChainMemoryPoolTransactionOutputProvider {
			chain: chain,
		}
	}
}

impl PreviousTransactionOutputProvider for ChainMemoryPoolTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.chain.read().memory_pool().previous_transaction_output(prevout)
	}

	fn is_spent(&self, prevout: &OutPoint) -> bool {
		self.chain.read().storage().transaction_meta(&prevout.hash).and_then(|tm| Some(tm.is_spent(prevout.index as usize))).unwrap_or(false)
	}
}

impl PreviousTransactionOutputProvider for EmptyTransactionOutputProvider {
	fn previous_transaction_output(&self, _prevout: &OutPoint) -> Option<TransactionOutput> {
		None
	}

	fn is_spent(&self, prevout: &OutPoint) -> bool {
		false
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::HashMap;
	use parking_lot::Mutex;
	use chain::Transaction;
	use synchronization_client::SynchronizationClientCore;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use super::{Verifier, VerificationSink};
	use db::IndexedBlock;

	#[derive(Default)]
	pub struct DummyVerifier {
		sink: Option<Arc<Mutex<SynchronizationClientCore<DummyTaskExecutor>>>>,
		errors: HashMap<H256, String>
	}

	impl DummyVerifier {
		pub fn set_sink(&mut self, sink: Arc<Mutex<SynchronizationClientCore<DummyTaskExecutor>>>) {
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
					Some(err) => sink.lock().on_block_verification_error(&err, &block.hash()),
					None => {
						sink.lock().on_block_verification_success(block);
						()
					},
				},
				None => panic!("call set_sink"),
			}
		}

		fn verify_transaction(&self, _height: u32, transaction: Transaction) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&transaction.hash()) {
					Some(err) => sink.lock().on_transaction_verification_error(&err, &transaction.hash()),
					None => sink.lock().on_transaction_verification_success(transaction),
				},
				None => panic!("call set_sink"),
			}
		}
	}
}
