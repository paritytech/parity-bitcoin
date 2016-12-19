use std::collections::VecDeque;
use std::sync::Arc;
use time;
use chain::{IndexedBlock, IndexedTransaction};
use db::{TransactionOutputObserver, PreviousTransactionOutputProvider};
use types::ChainRef;
use verification::{ChainVerifier, Chain as VerificationChain};

/// Verification task
pub enum VerificationTask {
}

/// Ordered verification tasks
pub type VerificationTasks = VecDeque<VerificationTask>;

/// Block verifier
pub trait BlockVerifier {
	/// Verify block
	fn verify_block(&self, block: IndexedBlock);
}

/// Transaction verifier
pub trait TransactionVerifier {
	/// Verify block
	fn verify_transaction(&self, block: IndexedTransaction);
}

/// Verifier
pub trait Verifier: BlockVerifier + TransactionVerifier {
}

/// Block verification events sink
pub trait BlockVerifierSink {
	/// When block verification has completed successfully.
	fn on_block_verification_success(&self, block: IndexedBlock) -> Option<Vec<VerificationTask>>;
	/// When block verification has failed.
	fn on_block_verification_error(&self, err: &str, block: IndexedBlock);
}

/// Transaction verification events sink
pub trait TransactionVerifierSink {
	/// When transaction verification has completed successfully.
	fn on_transaction_verification_success(&self, tx: IndexedTransaction);
	/// When transaction verification has failed.
	fn on_transaction_verification_error(&self, err: &str, tx: IndexedTransaction);
}

/// Verification events sink
pub trait VerifierSink: BlockVerifierSink + TransactionVerifierSink {
}

/// Asynchronous verifier implementation
pub struct AsyncVerifierImpl {
}

/// Synchronous verifier implementation
pub struct SyncVerifierImpl {
	/// Verifier itself
	verifier: ChainVerifier,
	/// Synchronization chain reference
	chain: ChainRef,
}

impl BlockVerifier for AsyncVerifierImpl {
	fn verify_block(&self, block: IndexedBlock) {
		self.work_sender
			.send(VerificationTask::VerifyBlock(block))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

impl TransactionVerifier for AsyncVerifierImpl {
	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: IndexedTransaction) {
		self.work_sender
			.send(VerificationTask::VerifyTransaction(height, transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

impl Verifier for AsyncVerifierImpl {
}

impl BlockVerifier for SyncVerifierImpl {
	fn verify_block(&self, block: IndexedBlock) {
		self.work_sender
			.send(VerificationTask::VerifyBlock(block))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

impl TransactionVerifier for SyncVerifierImpl {
	/// Verify transaction
	fn verify_transaction(&self, height: u32, transaction: IndexedTransaction) {
		self.work_sender
			.send(VerificationTask::VerifyTransaction(height, transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

impl Verifier for SyncVerifierImpl {
}

/// TODO: use SyncVerifier by AsyncVerifier => get rid of this method Execute single verification task
fn execute_verification_task<T: VerifierSink, U: TransactionOutputObserver + PreviousTransactionOutputProvider>(
	sink: &Arc<T>,
	tx_output_provider: Option<&U>,
	verifier: &ChainVerifier,
	task: VerificationTask
) {
	let mut tasks_queue: VecDeque<VerificationTask> = VecDeque::new();
	tasks_queue.push_back(task);

	while let Some(task) = tasks_queue.pop_front() {
		match task {
			VerificationTask::VerifyBlock(block) => {
				// verify block
				match verifier.verify(&block) {
					Ok(VerificationChain::Main) | Ok(VerificationChain::Side) => {
						if let Some(tasks) = sink.on_block_verification_success(block) {
							tasks_queue.extend(tasks);
						}
					},
					Ok(VerificationChain::Orphan) => {
						// this can happen for B1 if B0 verification has failed && we have already scheduled verification of B0
						sink.on_block_verification_error(&format!("orphaned block because parent block verification has failed"), &block.hash())
					},
					Err(e) => {
						sink.on_block_verification_error(&format!("{:?}", e), &block.hash())
					}
				}
			},
			VerificationTask::VerifyTransaction(height, transaction) => {
				let time: u32 = time::get_time().sec as u32;
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
