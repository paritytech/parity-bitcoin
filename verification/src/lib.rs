//! Bitcoin blocks verification

extern crate byteorder;
extern crate parking_lot;
extern crate linked_hash_map;
extern crate time;
#[macro_use]
extern crate log;

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
mod compact;
mod utils;

pub use primitives::{uint, hash};

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
	/// Block transactions are not final.
	NonFinalBlock,
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
	/// Too many signature operations
	Sigops(usize),
	/// Too many signature operations once p2sh operations included
	SigopsP2SH(usize),
	/// Coinbase transaction is found at position that is not 0
	MisplacedCoinbase(usize),
	/// Not fully spent transaction with the same hash already exists, bip30.
	UnspentTransactionWithTheSameHash,
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
	fn verify(&self, block: &db::IndexedBlock) -> VerificationResult;
}

/// Trait for verifier that can be interrupted and continue from the specific point
pub trait ContinueVerify : Verify + Send + Sync {
	type State;
	fn continue_verify(&self, block: &db::IndexedBlock, state: Self::State) -> VerificationResult;
}
