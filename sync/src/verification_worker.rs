use std::thread;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};
use chain::{Block, RepresentH256};
use synchronization_chain::ChainRef;
use db;
use verification::{Verify, ChainVerifier};

/// Verification thread
pub struct VerificationWorker {
	/// Transmission channel
	work_sender: Sender<VerificationTask>,
	/// Verification thread
	worker_thread: thread::JoinHandle<()>,
}

/// Verification thread tasks
enum VerificationTask {
	/// Verify single lock
	VerifyBlock(Block),
	/// Stop verification thread
	Stop,
}

impl VerificationWorker {
	pub fn new(storage: Arc<db::Store>, chain: ChainRef) -> Self {
		let (sender, receiver) = channel();
		let worker = VerificationWorker {
			work_sender: sender,
			worker_thread: thread::Builder::new()
				.name("Sync verification thread".to_string())
				.spawn(move || {
					VerificationWorker::worker_proc(receiver, chain, storage)
				})
				.expect("Error creating verification thread"),
		};
		worker
	}

	pub fn verify_block(&self, block: Block) {
		self.work_sender.send(VerificationTask::VerifyBlock(block)).expect("TODO");
	}

	fn worker_proc(work_receiver: Receiver<VerificationTask>, chain: ChainRef, storage: Arc<db::Store>) {
		let verifier = ChainVerifier::new(storage);
		while let Ok(task) = work_receiver.recv() {
			match task {
				VerificationTask::VerifyBlock(block) => {
					let hash = block.hash();
					match verifier.verify(&block) {
						Ok(_chain) => {
							chain.write().on_block_verification_success(hash, block);
						},
						Err(err) => {
							trace!(target: "sync", "Error verifying block {:?}: {:?}", hash, err);
							chain.write().on_block_verification_error(hash);
						}
					}
				},
				_ => break,
			}
		}
	}
}

impl Drop for VerificationWorker {
	fn drop(&mut self) {
		// TODO: it won't stop until all scheduled block are verified. Bad
		self.work_sender.send(VerificationTask::Stop).expect("TODO");
		//self.worker_thread.join().expect("Clean shutdown.");
	}
}