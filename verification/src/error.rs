use hash::H256;
use compact::Compact;
use storage::Error as DBError;
use script::Error as SignatureError;

#[derive(Debug, PartialEq)]
/// All possible verification errors
pub enum Error {
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
	/// Coinbase has invalid script sig prefix (BIP90 -> BIP34)
	CoinbaseScript,
	/// Maximum sigops operations exceeded - will not provide how much it was in total
	/// since it stops counting once `MAX_BLOCK_SIGOPS` is reached
	MaximumSigops,
	/// Maximum sigops operations cost  exceeded
	MaximumSigopsCost,
	/// Coinbase signature is not in the range 2-100
	CoinbaseSignatureLength(usize),
	/// Block size is invalid
	Size(usize),
	/// Block weight is invalid
	Weight,
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
	/// SegWit: bad witess nonce size
	WitnessInvalidNonceSize,
	/// SegWit: witness merkle mismatch
	WitnessMerkleCommitmentMismatch,
	/// SegWit: unexpected witness
	UnexpectedWitness,
	/// Non-canonical tranasctions ordering within block
	NonCanonicalTransactionOrdering,
	/// Database error
	Database(DBError),
}

impl From<DBError> for Error {
	fn from(err: DBError) -> Self {
		Error::Database(err)
	}
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
	/// Transaction size is below min size limit
	MinSize,
	/// Transaction has more sigops than it's allowed
	MaxSigops,
	/// Transaction is a part of memory pool, but is a coinbase
	MemoryPoolCoinbase,
	/// Not found corresponding output for transaction input
	Input(usize),
	/// Referenced coinbase output for the transaction input is not mature enough
	Maturity,
	/// Signature invalid for given input
	Signature(usize, SignatureError),
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
	/// Transaction, protected using BitcoinCash OP_RETURN replay protection (REQ-6-1).
	ReturnReplayProtection,
	/// Transaction with witness is received before SegWit is activated.
	PrematureWitness,
}

