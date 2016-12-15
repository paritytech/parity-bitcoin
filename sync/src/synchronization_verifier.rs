use std::thread;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use chain::{Transaction, OutPoint, TransactionOutput, IndexedBlock};
use network::Magic;
use miner::{DoubleSpendCheckResult, NonFinalDoubleSpendSet};
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
	/// In-storage checker
	storage_provider: StorageTransactionOutputProvider,
	/// Chain reference
	chain: ChainRef,
	/// Previous outputs, for which we should return 'Not spent' value.
	/// These are used when new version of transaction is received.
	nonfinal_spends: Option<NonFinalDoubleSpendSet>,
}

/// Transaction output observer, which looks into storage
struct StorageTransactionOutputProvider {
	/// Storage reference
	storage: SharedStore,
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
							Ok(prevout_provider) => prevout_provider,
						}
					} else {
						ChainMemoryPoolTransactionOutputProvider::for_block(chain.clone())
							.expect("no error when creating for block")
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
	/// Storage reference
	storage: SharedStore,
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
	fn verify_transaction(&self, _height: u32, _transaction: Transaction) {
		unimplemented!() // sync verifier is currently only used for blocks verification
	}
}

/// Execute single verification task
fn execute_verification_task<T: VerificationSink, U: TransactionOutputObserver + PreviousTransactionOutputProvider>(sink: &Arc<T>, tx_output_provider: &U, verifier: &ChainVerifier, task: VerificationTask) {
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
				match verifier.verify_mempool_transaction(tx_output_provider, height, time, &transaction) {
					Ok(_) => sink.on_transaction_verification_success(transaction),
					Err(e) => sink.on_transaction_verification_error(&format!("{:?}", e), &transaction.hash()),
				}
			},
			_ => unreachable!("must be checked by caller"),
		}
	}
}

impl StorageTransactionOutputProvider {
	pub fn with_storage(storage: SharedStore) -> Self {
		StorageTransactionOutputProvider {
			storage: storage,
		}
	}
}

impl TransactionOutputObserver for StorageTransactionOutputProvider {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		self.storage
			.transaction_meta(&prevout.hash)
			.and_then(|tm| tm.is_spent(prevout.index as usize))
	}
}

impl PreviousTransactionOutputProvider for StorageTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.storage.as_previous_transaction_output_provider().previous_transaction_output(prevout)
	}
}

impl ChainMemoryPoolTransactionOutputProvider {
	pub fn for_transaction(chain: ChainRef, transaction: &Transaction) -> Result<Self, verification::TransactionError> {
		// we have to check if there are another in-mempool transactions which spent same outputs here
		let check_result = chain.read().memory_pool().check_double_spend(transaction);
		ChainMemoryPoolTransactionOutputProvider::for_double_spend_check_result(chain, check_result)
	}

	pub fn for_block(chain: ChainRef) -> Result<Self, verification::TransactionError> {
		// we have to check if there are another in-mempool transactions which spent same outputs here
		let check_result = DoubleSpendCheckResult::NoDoubleSpend;
		ChainMemoryPoolTransactionOutputProvider::for_double_spend_check_result(chain, check_result)
	}

	fn for_double_spend_check_result(chain: ChainRef, check_result: DoubleSpendCheckResult) -> Result<Self, verification::TransactionError> {
		match check_result {
			DoubleSpendCheckResult::DoubleSpend(_, hash, index) => Err(verification::TransactionError::UsingSpentOutput(hash, index)),
			DoubleSpendCheckResult::NoDoubleSpend => Ok(ChainMemoryPoolTransactionOutputProvider {
				storage_provider: StorageTransactionOutputProvider::with_storage(chain.read().storage()),
				chain: chain.clone(),
				nonfinal_spends: None,
			}),
			DoubleSpendCheckResult::NonFinalDoubleSpend(nonfinal_spends) => Ok(ChainMemoryPoolTransactionOutputProvider {
				storage_provider: StorageTransactionOutputProvider::with_storage(chain.read().storage()),
				chain: chain.clone(),
				nonfinal_spends: Some(nonfinal_spends),
			}),
		}
	}
}

impl TransactionOutputObserver for ChainMemoryPoolTransactionOutputProvider {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		if let Some(ref nonfinal_spends) = self.nonfinal_spends {
			let prevout = prevout.clone().into();
			// check if this output is 'locked' by mempool transaction
			if nonfinal_spends.double_spends.contains(&prevout) {
				return Some(false);
			}
			// check if this output is output of transaction, which depends on locked mempool transaction
			if nonfinal_spends.dependent_spends.contains(&prevout) {
				return Some(false);
			}
 		}

		// we can omit memory_pool check here when we're verifying new transactions, because this
		// check has already been completed in `for_transaction` method
		// BUT if transactions are verifying because of reorganzation, we should check mempool
		// because while reorganizing, we can get new transactions to the mempool
		let chain = self.chain.read();
		if chain.memory_pool().is_spent(prevout) {
			return Some(true);
		}
		self.storage_provider.is_spent(prevout)
	}
}

impl PreviousTransactionOutputProvider for ChainMemoryPoolTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		// check if that is output of some transaction, which is vitually removed from memory pool
		if let Some(ref nonfinal_spends) = self.nonfinal_spends {
			if nonfinal_spends.dependent_spends.contains(&prevout.clone().into()) {
				// transaction is trying to replace some nonfinal transaction
				// + it is also depends on this transaction
				// => this is definitely an error
				return None;
			}
		}

		let chain = self.chain.read();
		chain.memory_pool().previous_transaction_output(prevout)
			.or_else(|| chain.storage().as_previous_transaction_output_provider().previous_transaction_output(prevout))
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::HashMap;
	use chain::{Transaction, OutPoint};
	use synchronization_chain::{Chain, ChainRef};
	use synchronization_client::CoreVerificationSink;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use chain::IndexedBlock;
	use super::{Verifier, BlockVerificationSink, TransactionVerificationSink, ChainMemoryPoolTransactionOutputProvider};
	use db::{self, TransactionOutputObserver, PreviousTransactionOutputProvider};
	use test_data;
	use parking_lot::RwLock;

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

	#[test]
	fn when_transaction_depends_on_removed_nonfinal_transaction() {
		let dchain = &mut test_data::ChainBuilder::new();

		test_data::TransactionBuilder::with_output(10).store(dchain)					// t0
			.reset().set_input(&dchain.at(0), 0).add_output(20).lock().store(dchain)	// nonfinal: t0[0] -> t1
			.reset().set_input(&dchain.at(1), 0).add_output(30).store(dchain)			// dependent: t0[0] -> t1[0] -> t2
			.reset().set_input(&dchain.at(0), 0).add_output(40).store(dchain);			// good replacement: t0[0] -> t3

		let mut chain = Chain::new(Arc::new(db::TestStorage::with_genesis_block()));
		chain.memory_pool_mut().insert_verified(dchain.at(0));
		chain.memory_pool_mut().insert_verified(dchain.at(1));
		chain.memory_pool_mut().insert_verified(dchain.at(2));

		// when inserting t3:
		// check that is_spent(t0[0]) == Some(false) (as it is spent by nonfinal t1)
		// check that is_spent(t1[0]) == Some(false) (as t1 is virtually removed)
		// check that is_spent(t2[0]) == Some(false) (as t2 is virtually removed)
		// check that previous_transaction_output(t0[0]) = Some(_)
		// check that previous_transaction_output(t1[0]) = None (as t1 is virtually removed)
		// check that previous_transaction_output(t2[0]) = None (as t2 is virtually removed)
		// =>
		// if t3 is also depending on t1[0] || t2[0], it will be rejected by verification as missing inputs
		let chain = ChainRef::new(RwLock::new(chain));
		let provider = ChainMemoryPoolTransactionOutputProvider::for_transaction(chain, &dchain.at(3)).unwrap();
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(0).hash(), index: 0, }), Some(false));
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(1).hash(), index: 0, }), Some(false));
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(2).hash(), index: 0, }), Some(false));
		assert_eq!(provider.previous_transaction_output(&OutPoint { hash: dchain.at(0).hash(), index: 0, }), Some(dchain.at(0).outputs[0].clone()));
		assert_eq!(provider.previous_transaction_output(&OutPoint { hash: dchain.at(1).hash(), index: 0, }), None);
		assert_eq!(provider.previous_transaction_output(&OutPoint { hash: dchain.at(2).hash(), index: 0, }), None);
	}
}
