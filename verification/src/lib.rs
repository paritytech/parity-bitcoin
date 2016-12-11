//! Bitcoin consensus verification
//!
//! --> A. on_new_block:
//!
//! A.1 VerifyHeader
//! A.2 VerifyBlock,
//! A.3 VerifyTransaction for each tx
//!
//! A.4.a if it is block from canon chain
//! A.4.a.1 AcceptHeader
//! A.4.a.2 AcceptBlock
//! A.4.a.3 AcceptTransaction for each tx
//!
//! A.4.b if it is block from side chain becoming canon
//! decanonize old canon chain blocks
//! canonize new canon chain blocks (without currently processed block)
//! A.4.b.1 AcceptHeader for each header
//! A.4.b.2 AcceptBlock for each block
//! A.4.b.3 AcceptTransaction for each tx in each block
//! A.4.b.4 AcceptHeader
//! A.4.b.5 AcceptBlock
//! A.4.b.6 AcceptTransaction for each tx
//! if any step failed, revert chain back to old canon
//!
//! A.4.c if it is block from side chain do nothing
//!
//! --> B. on_memory_pool_transaction
//!
//! B.1 VerifyMemoryPoolTransaction
//! B.2 AcceptMemoryPoolTransaction
//!
//! --> C. on_block_header
//!
//! C.1 VerifyHeader
//! C.2 AcceptHeader (?)
//!
//! --> D. after successfull chain_reorganization
//!
//! D.1 AcceptMemoryPoolTransaction on each tx in memory pool
//!
//! --> E. D might be super inefficient when memory pool is large
//! so instead we might want to call AcceptMemoryPoolTransaction on each tx
//! that is inserted into assembled block

extern crate parking_lot;
extern crate time;
#[macro_use]
extern crate log;
extern crate scoped_pool;
extern crate rayon;

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

pub mod constants;
mod duplex_store;
mod canon;
mod accept_block;
mod accept_chain;
mod accept_header;
mod accept_transaction;
mod verify_block;
mod verify_chain;
mod verify_header;
mod verify_transaction;

pub use primitives::{uint, hash, compact};

pub use canon::{CanonBlock, CanonHeader, CanonTransaction};

pub use accept_block::BlockAcceptor;
pub use accept_chain::ChainAcceptor;
pub use accept_header::HeaderAcceptor;
pub use accept_transaction::TransactionAcceptor;

pub use verify_block::BlockVerifier;
pub use verify_chain::ChainVerifier as XXXChainVerifier;
pub use verify_header::HeaderVerifier;
pub use verify_transaction::{TransactionVerifier, MemoryPoolTransactionVerifier};

pub use chain_verifier::{Chain, ChainVerifier, VerificationResult};
pub use error::{Error, TransactionError};
pub use sigops::{transaction_sigops, StoreWithUnretainedOutputs};
pub use utils::{work_required, is_valid_proof_of_work, is_valid_proof_of_work_hash, block_reward_satoshi};

/// Interface for block verification
pub trait Verify : Send + Sync {
	fn verify(&self, block: &db::IndexedBlock) -> VerificationResult;
}
