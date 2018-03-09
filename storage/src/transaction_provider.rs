use hash::H256;
use bytes::Bytes;
use chain::{Transaction, OutPoint, TransactionOutput};
use {TransactionMeta};

/// Should be used to obtain all transactions from canon chain and forks.
pub trait TransactionProvider {
	/// Returns true if store contains given transaction.
	fn contains_transaction(&self, hash: &H256) -> bool {
		self.transaction(hash).is_some()
	}

	/// Resolves transaction body bytes by transaction hash.
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// Resolves serialized transaction info by transaction hash.
	fn transaction(&self, hash: &H256) -> Option<Transaction>;
}

/// Should be used to get canon chain transaction outputs.
pub trait TransactionOutputProvider: Send + Sync {
	/// Returns transaction output.
	fn transaction_output(&self, outpoint: &OutPoint, transaction_index: usize) -> Option<TransactionOutput>;

	/// Returns true if we know that output is double spent.
	fn is_spent(&self, outpoint: &OutPoint) -> bool;
}

/// Transaction meta provider stores transaction meta information
pub trait TransactionMetaProvider: Send + Sync {
	/// Returns None if transactin with given hash does not exist
	/// Otherwise returns transaction meta object
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}
