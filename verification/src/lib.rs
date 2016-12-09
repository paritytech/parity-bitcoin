//! Bitcoin blocks verification

extern crate byteorder;
extern crate parking_lot;
extern crate time;
#[macro_use]
extern crate log;
extern crate scoped_pool;

extern crate db;
extern crate chain;
extern crate network;
extern crate primitives;
extern crate serialization;
extern crate script;

#[cfg(test)]
extern crate ethcore_devtools as devtools;
#[cfg(test)]
extern crate test_data;

mod chain_verifier;
mod error;
mod sigops;
mod task;
mod utils;

pub use primitives::{uint, hash, compact};

pub use chain_verifier::{Chain, ChainVerifier, VerificationResult, MAX_BLOCK_SIZE, MAX_BLOCK_SIGOPS};
pub use error::{Error, TransactionError};
pub use sigops::{transaction_sigops, StoreWithUnretainedOutputs};
pub use utils::{work_required, is_valid_proof_of_work, is_valid_proof_of_work_hash, block_reward_satoshi};

/// Interface for block verification
pub trait Verify : Send + Sync {
	fn verify(&self, block: &db::IndexedBlock) -> VerificationResult;
}
