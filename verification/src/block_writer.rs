//! Parallel block writer

use queue::{Queue, WorkStatus};
use db;
use insert_verifier::InsertVerifier;
use chain_verifier::ChainVerifier;
use super::{Verify, Chain};
use devtools::StopGuard;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::{thread, time};

const VERIFICATION_THREADS: usize = 4;

pub struct BlockWriter {
	store: Arc<db::Store>,
	queue: Arc<Queue>,
	_stop: StopGuard,
}

impl BlockWriter {

	pub fn new(store: Arc<db::Store>) -> BlockWriter {
		let chain_verifier = ChainVerifier::new(store.clone());
		let queue = Arc::new(Queue::new(Box::new(chain_verifier)));
		let guard = StopGuard::new();

		// parallel verification work stealers
		for _ in 0..VERIFICATION_THREADS {
			let thread_stop = guard.share();
			let thread_queue = queue.clone();
			thread::spawn(move ||
				{
					while !thread_stop.load(Ordering::Relaxed) {
						if thread_queue.process() == WorkStatus::Wait {
							// nothing to verify
							thread::park_timeout(time::Duration::from_millis(100));
						}
					}
				});
		}

		{
			// synchronous insertion
			let thread_stop = guard.share();
			let thread_store = store.clone();
			let thread_queue = queue.clone();
			thread::spawn(move || {
				while !thread_stop.load(Ordering::Relaxed) {
					let insert_verifier = InsertVerifier::new(thread_store.clone());
					if thread_queue.process_valid(|_, verified_block| {
						match insert_verifier.verify(&verified_block.block) {
							Ok(Chain::Main) | Ok(Chain::Side) => {
								if let Err(e) = thread_store.insert_block(&verified_block.block) {
									panic!("Database block insert failed: {:?}", e);
								}
							},
							Ok(Chain::Orphan) => {
								// todo: handle or drop orphan
							},
							Err(_) => {
								// todo: warning
							}
						};
					}) == WorkStatus::Wait {
						// nothing to insert
						thread::park_timeout(time::Duration::from_millis(100));
					}
				}
			});
		}

		BlockWriter {
			store: store,
			queue: queue,
			_stop: guard,
		}
	}

	pub fn queue(&self) -> &Queue {
		&*self.queue
	}

	pub fn store(&self) -> &Arc<db::Store> {
		&self.store
	}
}
