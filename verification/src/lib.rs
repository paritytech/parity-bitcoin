//! Bitcoin blocks verification

extern crate db;
extern crate primitives;
extern crate chain;
extern crate serialization;
extern crate parking_lot;
extern crate linked_hash_map;
extern crate byteorder;
extern crate time;
extern crate script;
#[macro_use]
extern crate log;

#[cfg(test)]
extern crate ethcore_devtools as devtools;
#[cfg(test)]
extern crate test_data;

mod queue;
mod utils;
mod chain_verifier;

pub use queue::Queue;
pub use chain_verifier::ChainVerifier;

use primitives::hash::H256;

#[derive(Debug, PartialEq)]
/// All possible verification errors
pub enum Error {
	/// has an equal duplicate in the chain
	Duplicate,
	/// No transactions in block
	Empty,
	/// Invalid proof-of-work (Block hash does not satisfy nBits)
	Pow,
	/// Invalid timestamp
	Timestamp,
	/// First transaction is not a coinbase transaction
	Coinbase,
	/// One of the transactions is invalid (corresponding index and specific transaction error)
	Transaction(usize, TransactionError),
	/// nBits do not match difficulty rules
	Difficulty,
	/// Invalid merkle root
	MerkleRoot,
	/// Coinbase spends too much
	CoinbaseOverspend { expected_max: u64, actual: u64 },
	/// Maximum sigops operations exceeded - will not provide how much it was in total
	/// since it stops counting once `MAX_BLOCK_SIGOPS` is reached
	MaximumSigops,
	/// Coinbase signature is not in the range 2-100
	CoinbaseSignatureLength(usize),
	/// Block size is invalid
	Size(usize),
}

#[derive(Debug, PartialEq)]
/// Possible transactions verification errors
pub enum TransactionError {
	/// Not found corresponding output for transaction input
	Input(usize),
	/// Referenced coinbase output for the transaction input is not mature enough
	Maturity,
	/// Signature invalid for given input
	Signature(usize),
	/// Inconclusive (unknown parent transaction)
	Inconclusive(H256),
	/// Unknown previous transaction referenced
	UnknownReference(H256),
	/// Spends more than claims
	Overspend,
	/// Signature script can't be properly parsed
	SignatureMallformed(String),
}

#[derive(PartialEq, Debug)]
/// Block verification chain
pub enum Chain {
	/// Main chain
	Main,
	/// Side chain
	Side,
	/// Orphan (no known parent)
	Orphan,
}

#[derive(PartialEq, Debug)]
/// block status within the queue
pub enum BlockStatus {
	Valid,
	Invalid,
	Pending,
	Absent,
	Verifying,
}

/// Verification result
pub type VerificationResult = Result<Chain, Error>;

/// Interface for block verification
pub trait Verify : Send + Sync {
	fn verify(&self, block: &chain::Block) -> VerificationResult;
}

/// Trait for verifier that can be interrupted and continue from the specific point
pub trait ContinueVerify : Verify + Send + Sync {
	type State;
	fn continue_verify(&self, block: &chain::Block, state: Self::State) -> VerificationResult;
}
