use std::thread;
use std::collections::{VecDeque, HashSet};
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use chain::{Transaction, OutPoint, TransactionOutput, IndexedBlock};
use network::Magic;
use miner::{HashedOutPoint, DoubleSpendCheckResult};
use primitives::hash::H256;
use synchronization_chain::ChainRef;
use verification::{self, BackwardsCompatibleChainVerifier as ChainVerifier, Verify as VerificationVerify, Chain};
use db::{SharedStore, PreviousTransactionOutputProvider, TransactionOutputObserver};
use time::get_time;

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
	fn on_transaction_verification_success(&self, transaction: Transaction);
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

/// Transaction output observer, which looks into storage && into memory pool
struct ChainMemoryPoolTransactionOutputProvider {
	/// Chain reference
	chain: ChainRef,
	/// Previous outputs, for which we should return 'Not spent' value.
	/// These are used when new version of transaction is received.
	locked_outputs: Option<HashSet<HashedOutPoint>>,
}

impl VerificationTask {
	/// Returns transaction reference if it is transaction verification task
	pub fn transaction(&self) -> Option<&Transaction> {
		match self {
			&VerificationTask::VerifyTransaction(_, ref transaction) => Some(&transaction),
			_ => None,
		}
	}
}

impl AsyncVerifier {
	/// Create new async verifier
	pub fn new<T: VerificationSink>(verifier: Arc<ChainVerifier>, chain: ChainRef, sink: Arc<T>) -> Self {
		let (verification_work_sender, verification_work_receiver) = channel();
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
	fn verification_worker_proc<T: VerificationSink>(sink: Arc<T>, chain: ChainRef, verifier: Arc<ChainVerifier>, work_receiver: Receiver<VerificationTask>) {
		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::Stop => break,
				_ => {
					let prevout_provider = if let Some(ref transaction) = task.transaction() {
						match ChainMemoryPoolTransactionOutputProvider::for_transaction(chain.clone(), transaction) {
							Err(e) => {
								sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash());
								return;
							},
							Ok(prevout_provider) => Some(prevout_provider),
						}
					} else {
						None
					};
					let prevout_provider = match prevout_provider {
						Some(ref prevout_provider) => Some(prevout_provider),
						None => None,
					};
					execute_verification_task(&sink, prevout_provider, &verifier, task)
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
	sink: Arc<T>,
}

impl<T> SyncVerifier<T> where T: VerificationSink {
	/// Create new sync verifier
	pub fn new(network: Magic, storage: SharedStore, sink: Arc<T>) -> Self {
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
		execute_verification_task::<T, ChainMemoryPoolTransactionOutputProvider>(&self.sink, None, &self.verifier, VerificationTask::VerifyBlock(block))
	}

	/// Verify transaction
	fn verify_transaction(&self, _height: u32, _transaction: Transaction) {
		unimplemented!() // sync verifier is currently only used for blocks verification
	}
}

/// Execute single verification task
fn execute_verification_task<T: VerificationSink, U: TransactionOutputObserver + PreviousTransactionOutputProvider>(sink: &Arc<T>, tx_output_provider: Option<&U>, verifier: &ChainVerifier, task: VerificationTask) {
	let mut tasks_queue: VecDeque<VerificationTask> = VecDeque::new();
	tasks_queue.push_back(task);

	while let Some(task) = tasks_queue.pop_front() {
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
				let tx_output_provider = tx_output_provider.expect("must be provided for transaction checks");
				match verifier.verify_mempool_transaction(tx_output_provider, height, time, &transaction) {
					Ok(_) => sink.on_transaction_verification_success(transaction),
					Err(e) => sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash()),
				}
			},
			_ => unreachable!("must be checked by caller"),
		}
	}
}

impl ChainMemoryPoolTransactionOutputProvider {
	pub fn for_transaction(chain: ChainRef, transaction: &Transaction) -> Result<Self, verification::TransactionError> {
		// we have to check if there are another in-mempool transactions which spent same outputs here
		match chain.read().memory_pool().check_double_spend(transaction) {
			DoubleSpendCheckResult::DoubleSpend(_, hash, index) => Err(verification::TransactionError::UsingSpentOutput(hash, index)),
			DoubleSpendCheckResult::NoDoubleSpend => Ok(ChainMemoryPoolTransactionOutputProvider {
				chain: chain.clone(),
				locked_outputs: None,
			}),
			DoubleSpendCheckResult::LockedDoubleSpend(locked_outputs) => Ok(ChainMemoryPoolTransactionOutputProvider {
				chain: chain.clone(),
				locked_outputs: Some(locked_outputs),
			}),
		}
	}
}

impl TransactionOutputObserver for ChainMemoryPoolTransactionOutputProvider {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		// check if this output is 'locked' by mempool transaction
		if let Some(ref locked_outputs) = self.locked_outputs {
			if locked_outputs.contains(&prevout.clone().into()) {
				return Some(false);
			}
		}

		// check spending in storage
		self.chain.read().storage().transaction_meta(&prevout.hash).and_then(|tm| tm.is_spent(prevout.index as usize))
	}
}

impl PreviousTransactionOutputProvider for ChainMemoryPoolTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		let chain = self.chain.read();
		chain.memory_pool().previous_transaction_output(prevout)
			.or_else(|| chain.storage().as_previous_transaction_output_provider().previous_transaction_output(prevout))
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::HashMap;
	use chain::Transaction;
	use synchronization_chain::Chain;
	use synchronization_client::CoreVerificationSink;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use chain::IndexedBlock;
	use super::{Verifier, BlockVerificationSink, TransactionVerificationSink};
	use db;
	use test_data;

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

		fn verify_transaction(&self, _height: u32, transaction: Transaction) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&transaction.hash()) {
					Some(err) => sink.on_transaction_verification_error(&err, &transaction.hash()),
					None => sink.on_transaction_verification_success(transaction),
				},
				None => panic!("call set_sink"),
			}
		}
	}

	#[test]
	fn when_transaction_spends_output_twice() {
		let tx1: Transaction = test_data::TransactionBuilder::with_default_input(0).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_default_input(1).into();
		let out1 = tx1.inputs[0].previous_output.clone();
		let out2 = tx2.inputs[0].previous_output.clone();
		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		chain.memory_pool_mut().insert_verified(tx1);
		assert!(chain.memory_pool().is_spent(&out1));
		assert!(!chain.memory_pool().is_spent(&out2));
	}
}
