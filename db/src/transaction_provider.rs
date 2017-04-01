use hash::H256;
use bytes::Bytes;
use chain::{Transaction, OutPoint, TransactionOutput};

pub trait TransactionProvider {
	/// returns true if store contains given transaction
	fn contains_transaction(&self, hash: &H256) -> bool {
		self.transaction(hash).is_some()
	}

	/// resolves transaction body bytes by transaction hash
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// resolves serialized transaction info by transaction hash
	fn transaction(&self, hash: &H256) -> Option<Transaction>;
}

/// During transaction verifiction the only part of old transaction that we need is `TransactionOutput`.
/// Structures like `IndexedBlock` or `MemoryPool` already have it in memory, so it would be
/// a shame to clone the whole transaction just to get single output.
pub trait PreviousTransactionOutputProvider: Send + Sync {
	fn previous_transaction_output(&self, prevout: &OutPoint, transaction_index: usize) -> Option<TransactionOutput>;
}
