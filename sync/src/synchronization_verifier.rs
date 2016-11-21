use std::thread;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
use chain::{Block, Transaction, RepresentH256};
use message::common::ConsensusParams;
use primitives::hash::H256;
use verification::{ChainVerifier, Verify as VerificationVerify};
use synchronization_chain::ChainRef;

/// Verification events sink
pub trait VerificationSink : Send + 'static {
	/// When block verification has completed successfully.
	fn on_block_verification_success(&mut self, block: Block);
	/// When block verification has failed.
	fn on_block_verification_error(&mut self, err: &str, hash: &H256);
	/// When transaction verification has completed successfully.
	fn on_transaction_verification_success(&mut self, transaction: Transaction);
	/// When transaction verification has failed.
	fn on_transaction_verification_error(&mut self, err: &str, hash: &H256);
}

/// Verification thread tasks
enum VerificationTask {
	/// Verify single block
	VerifyBlock(Block),
	/// Verify single transaction
	VerifyTransaction(Transaction),
	/// Stop verification thread
	Stop,
}

/// Synchronization verifier
pub trait Verifier : Send + 'static {
	/// Verify block
	fn verify_block(&self, block: Block);
	/// Verify transaction
	fn verify_transaction(&self, transaction: Transaction);
}

/// Asynchronous synchronization verifier
pub struct AsyncVerifier {
	/// Verification work transmission channel.
	verification_work_sender: Sender<VerificationTask>,
	/// Verification thread.
	verification_worker_thread: Option<thread::JoinHandle<()>>,
}

impl AsyncVerifier {
	/// Create new async verifier
	pub fn new<T: VerificationSink>(consensus_params: ConsensusParams, chain: ChainRef, sink: Arc<Mutex<T>>) -> Self {
		let (verification_work_sender, verification_work_receiver) = channel();
		let storage = chain.read().storage();
		let verifier = ChainVerifier::new(storage);
		AsyncVerifier {
			verification_work_sender: verification_work_sender,
			verification_worker_thread: Some(thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					AsyncVerifier::verification_worker_proc(sink, chain, consensus_params, verifier, verification_work_receiver)
				})
				.expect("Error creating verification thread"))
		}
	}

	/// Thread procedure for handling verification tasks
	fn verification_worker_proc<T: VerificationSink>(sink: Arc<Mutex<T>>, chain: ChainRef, consensus_params: ConsensusParams, mut verifier: ChainVerifier, work_receiver: Receiver<VerificationTask>) {
		let bip16_time_border = consensus_params.bip16_time;
		let mut is_bip16_active = false;
		let mut parameters_change_steps = Some(0);

		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::VerifyBlock(block) => {
					// for changes that are not relying on block#
					let is_bip16_active_on_block = block.block_header.time >= bip16_time_border;
					let force_parameters_change = is_bip16_active_on_block != is_bip16_active;
					if force_parameters_change {
						parameters_change_steps = Some(0);
					}

					// change verifier parameters, if needed
					if let Some(steps_left) = parameters_change_steps {
						if steps_left == 0 {
							let best_storage_block = chain.read().best_storage_block();

							is_bip16_active = is_bip16_active_on_block;
							verifier = verifier.verify_p2sh(is_bip16_active);

							let is_bip65_active = best_storage_block.number >= consensus_params.bip65_height;
							verifier = verifier.verify_clocktimeverify(is_bip65_active);

							if is_bip65_active {
								parameters_change_steps = None;
							} else {
								parameters_change_steps = Some(consensus_params.bip65_height - best_storage_block.number);
							}
						} else {
							parameters_change_steps = Some(steps_left - 1);
						}
					}

					// verify block
					match verifier.verify(&block) {
						Ok(_chain) => {
							sink.lock().on_block_verification_success(block)
						},
						Err(e) => {
							sink.lock().on_block_verification_error(&format!("{:?}", e), &block.hash())
						}
					}
				},
				VerificationTask::VerifyTransaction(transaction) => {
					// TODO: add verification here
					sink.lock().on_transaction_verification_error("unimplemented", &transaction.hash())
				}
				VerificationTask::Stop => break,
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
	fn verify_block(&self, block: Block) {
		self.verification_work_sender
			.send(VerificationTask::VerifyBlock(block))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}

	/// Verify transaction
	fn verify_transaction(&self, transaction: Transaction) {
		self.verification_work_sender
			.send(VerificationTask::VerifyTransaction(transaction))
			.expect("Verification thread have the same lifetime as `AsyncVerifier`");
	}
}

#[cfg(test)]
pub mod tests {
	use std::sync::Arc;
	use std::collections::HashMap;
	use parking_lot::Mutex;
	use chain::{Block, Transaction, RepresentH256};
	use synchronization_client::SynchronizationClientCore;
	use synchronization_executor::tests::DummyTaskExecutor;
	use primitives::hash::H256;
	use super::{Verifier, VerificationSink};

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
		fn verify_block(&self, block: Block) {
			match self.sink {
				Some(ref sink) => match self.errors.get(&block.hash()) {
					Some(err) => sink.lock().on_block_verification_error(&err, &block.hash()),
					None => sink.lock().on_block_verification_success(block),
				},
				None => panic!("call set_sink"),
			}
		}

		fn verify_transaction(&self, transaction: Transaction) {
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
