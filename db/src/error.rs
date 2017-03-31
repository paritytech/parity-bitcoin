use primitives::hash::H256;
use primitives::compact::Compact;
use std;

#[derive(Debug)]
/// Database error
pub enum Error {
	/// Rocksdb error
	DB(String),
	/// Io error
	Io(std::io::Error),
	/// Invalid meta info (while opening the database)
	Meta(MetaError),
	/// Database blockchain consistency error
	Consistency(ConsistencyError),
}

impl Error {
	pub fn unknown_hash(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::Unknown(h.clone()))
	}

	pub fn unknown_number(n: u32) -> Self {
		Error::Consistency(ConsistencyError::UnknownNumber(n))
	}

	pub fn unknown_spending(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::UnknownSpending(h.clone()))
	}

	pub fn double_spend(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::DoubleSpend(h.clone()))
	}

	pub fn not_main(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::NotMain(h.clone()))
	}

	pub fn reorganize(h: &H256) -> Self {
		Error::Consistency(ConsistencyError::Reorganize(h.clone()))
	}
}

#[derive(Debug, PartialEq)]
pub enum ConsistencyError {
	/// Unknown hash
	Unknown(H256),
	/// Unknown number
	UnknownNumber(u32),
	/// Not the block from the main chain
	NotMain(H256),
	/// Fork too long
	ForkTooLong,
	/// Main chain block transaction attempts to double-spend
	DoubleSpend(H256),
	/// Transaction tries to spend
	UnknownSpending(H256),
	/// Chain has no best block
	NoBestBlock,
	/// Failed reorganization caused by block
	Reorganize(H256),
}


#[derive(Debug, PartialEq)]
pub enum MetaError {
	UnsupportedVersion,
}

impl From<String> for Error {
	fn from(err: String) -> Error {
		Error::DB(err)
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Error {
		Error::Io(err)
	}
}

#[derive(Debug, PartialEq)]
/// All possible verification errors
pub enum VerificationError {
	/// has an equal duplicate in the chain
	Duplicate,
	/// Contains duplicated transactions
	DuplicatedTransactions,
	/// No transactions in block
	Empty,
	/// Invalid proof-of-work (Block hash does not satisfy nBits)
	Pow,
	/// Futuristic timestamp
	FuturisticTimestamp,
	/// Invalid timestamp
	Timestamp,
	/// First transaction is not a coinbase transaction
	Coinbase,
	/// One of the transactions is invalid (corresponding index and specific transaction error)
	Transaction(usize, TransactionError),
	/// nBits do not match difficulty rules
	Difficulty { expected: Compact, actual: Compact },
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
	/// Old version block.
	OldVersionBlock,
	/// Sum of the transaction fees in block + coinbase reward exceeds u64::max
	TransactionFeeAndRewardOverflow,
	/// Sum of the transaction fees in block exceeds u64::max
	TransactionFeesOverflow,
	/// Sum of all referenced outputs in block transactions resulted in the overflow
	ReferencedInputsSumOverflow,
}

#[derive(Debug, PartialEq)]
/// Possible transactions verification errors
pub enum TransactionError {
	/// Transaction has no inputs or no outputs
	Empty,
	/// Transaction is not coinbase transaction but has null inputs
	NullNonCoinbase,
	/// Coinbase signature is not in the range 2-100
	CoinbaseSignatureLength(usize),
	/// Transaction size exceeds block size limit
	MaxSize,
	/// Transaction has more sigops than it's allowed
	MaxSigops,
	/// Transaction is a part of memory pool, but is a coinbase
	MemoryPoolCoinbase,
	/// Not found corresponding output for transaction input
	Input(usize),
	/// Referenced coinbase output for the transaction input is not mature enough
	Maturity,
	/// Signature invalid for given input
	Signature(usize),
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
	MisplacedCoinbase,
	/// Not fully spent transaction with the same hash already exists, bip30.
	UnspentTransactionWithTheSameHash,
	/// Using output that is surely spent
	UsingSpentOutput(H256, u32),
}
