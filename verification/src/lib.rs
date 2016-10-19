//! Bitcoin blocks verification

extern crate db;
extern crate primitives;
extern crate chain;
extern crate serialization;
extern crate parking_lot;
extern crate linked_hash_map;

#[cfg(test)]
extern crate ethcore_devtools as devtools;
#[cfg(test)]
extern crate test_data;

mod queue;

pub use queue::Queue;

#[derive(Debug)]
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
	Difficulty
}

#[derive(Debug)]
/// Possible transactions verification errors
pub enum TransactionError {
	/// Not found corresponding output for transaction input
	Input,
	/// Referenced coinbase output for the transaction input is not mature enough
	Maturity,
	/// Signature invalid
	Signature,
}

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
