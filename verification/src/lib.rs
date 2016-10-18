//! Bitcoin blocks verification

extern crate db;
extern crate primitives;
extern crate chain;
extern crate serialization;
extern crate parking_lot;

#[cfg(test)]
extern crate ethcore_devtools as devtools;

use primitives::hash::H256;

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
	/// One of the transactions is invalid
	Transaction(TransactionError, H256),
	/// nBits does not match difficulty rules
	Difficulty
}

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

/// Verification result
pub type VerificationResult = Result<Chain, Error>;

/// Interface for block verification
pub trait Verify {
	fn verify(block: &chain::Block) -> Result<VerificationResult, Error>;
}
