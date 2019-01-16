use std::ops;
use ser::Serializable;
use chain::IndexedTransaction;
use network::{ConsensusParams, ConsensusFork};
use deployments::BlockDeployments;
use storage::NoopStore;
use sigops::transaction_sigops;
use error::TransactionError;
use constants::{MIN_COINBASE_SIZE, MAX_COINBASE_SIZE};

pub struct TransactionVerifier<'a> {
	pub empty: TransactionEmpty<'a>,
	pub null_non_coinbase: TransactionNullNonCoinbase<'a>,
	pub oversized_coinbase: TransactionOversizedCoinbase<'a>,
}

impl<'a> TransactionVerifier<'a> {
	pub fn new(transaction: &'a IndexedTransaction) -> Self {
		trace!(target: "verification", "Tx pre-verification {}", transaction.hash.to_reversed_str());
		TransactionVerifier {
			empty: TransactionEmpty::new(transaction),
			null_non_coinbase: TransactionNullNonCoinbase::new(transaction),
			oversized_coinbase: TransactionOversizedCoinbase::new(transaction, MIN_COINBASE_SIZE..MAX_COINBASE_SIZE),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		try!(self.empty.check());
		try!(self.null_non_coinbase.check());
		try!(self.oversized_coinbase.check());
		Ok(())
	}
}

pub struct MemoryPoolTransactionVerifier<'a> {
	pub empty: TransactionEmpty<'a>,
	pub null_non_coinbase: TransactionNullNonCoinbase<'a>,
	pub is_coinbase: TransactionMemoryPoolCoinbase<'a>,
	pub size: TransactionSize<'a>,
	pub premature_witness: TransactionPrematureWitness<'a>,
	pub sigops: TransactionSigops<'a>,
}

impl<'a> MemoryPoolTransactionVerifier<'a> {
	pub fn new(transaction: &'a IndexedTransaction, consensus: &'a ConsensusParams, deployments: &'a BlockDeployments<'a>) -> Self {
		trace!(target: "verification", "Mempool-Tx pre-verification {}", transaction.hash.to_reversed_str());
		MemoryPoolTransactionVerifier {
			empty: TransactionEmpty::new(transaction),
			null_non_coinbase: TransactionNullNonCoinbase::new(transaction),
			is_coinbase: TransactionMemoryPoolCoinbase::new(transaction),
			size: TransactionSize::new(transaction, consensus),
			premature_witness: TransactionPrematureWitness::new(transaction, &deployments),
			sigops: TransactionSigops::new(transaction, ConsensusFork::absolute_maximum_block_sigops()),
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		try!(self.empty.check());
		try!(self.null_non_coinbase.check());
		try!(self.is_coinbase.check());
		try!(self.size.check());
		try!(self.premature_witness.check());
		try!(self.sigops.check());
		Ok(())
	}
}

pub struct TransactionEmpty<'a> {
	transaction: &'a IndexedTransaction,
}

impl<'a> TransactionEmpty<'a> {
	fn new(transaction: &'a IndexedTransaction) -> Self {
		TransactionEmpty {
			transaction: transaction,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_empty() {
			Err(TransactionError::Empty)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionNullNonCoinbase<'a> {
	transaction: &'a IndexedTransaction,
}

impl<'a> TransactionNullNonCoinbase<'a> {
	fn new(transaction: &'a IndexedTransaction) -> Self {
		TransactionNullNonCoinbase {
			transaction: transaction,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if !self.transaction.raw.is_coinbase() && self.transaction.raw.is_null() {
			Err(TransactionError::NullNonCoinbase)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionOversizedCoinbase<'a> {
	transaction: &'a IndexedTransaction,
	size_range: ops::Range<usize>,
}

impl<'a> TransactionOversizedCoinbase<'a> {
	fn new(transaction: &'a IndexedTransaction, size_range: ops::Range<usize>) -> Self {
		TransactionOversizedCoinbase {
			transaction: transaction,
			size_range: size_range,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			let script_len = self.transaction.raw.inputs[0].script_sig.len();
			if script_len < self.size_range.start || script_len > self.size_range.end {
				return Err(TransactionError::CoinbaseSignatureLength(script_len));
			}
		}

		Ok(())
	}
}

pub struct TransactionMemoryPoolCoinbase<'a> {
	transaction: &'a IndexedTransaction,
}
impl<'a> TransactionMemoryPoolCoinbase<'a> {
	fn new(transaction: &'a IndexedTransaction) -> Self {
		TransactionMemoryPoolCoinbase {
			transaction: transaction,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		if self.transaction.raw.is_coinbase() {
			Err(TransactionError::MemoryPoolCoinbase)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionSize<'a> {
	transaction: &'a IndexedTransaction,
	consensus: &'a ConsensusParams,
}

impl<'a> TransactionSize<'a> {
	fn new(transaction: &'a IndexedTransaction, consensus: &'a ConsensusParams) -> Self {
		TransactionSize {
			transaction: transaction,
			consensus: consensus,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		let size = self.transaction.raw.serialized_size();
		if size > self.consensus.fork.max_transaction_size() {
			Err(TransactionError::MaxSize)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionSigops<'a> {
	transaction: &'a IndexedTransaction,
	max_sigops: usize,
}

impl<'a> TransactionSigops<'a> {
	fn new(transaction: &'a IndexedTransaction, max_sigops: usize) -> Self {
		TransactionSigops {
			transaction: transaction,
			max_sigops: max_sigops,
		}
	}

	fn check(&self) -> Result<(), TransactionError> {
		let sigops = transaction_sigops(&self.transaction.raw, &NoopStore, false, false);
		if sigops > self.max_sigops {
			Err(TransactionError::MaxSigops)
		} else {
			Ok(())
		}
	}
}

pub struct TransactionPrematureWitness<'a> {
	transaction: &'a IndexedTransaction,
	segwit_active: bool,
}

impl<'a> TransactionPrematureWitness<'a> {
	pub fn new(transaction: &'a IndexedTransaction, deployments: &'a BlockDeployments<'a>) -> Self {
		let segwit_active = deployments.segwit();

		TransactionPrematureWitness {
			transaction: transaction,
			segwit_active: segwit_active,
		}
	}

	pub fn check(&self) -> Result<(), TransactionError> {
		if !self.segwit_active && self.transaction.raw.has_witness() {
			Err(TransactionError::PrematureWitness)
		} else {
			Ok(())
		}
	}
}
