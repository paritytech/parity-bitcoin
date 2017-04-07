use primitives::hash::H256;
use chain::OutPoint;
use transaction_meta::TransactionMeta;

/// Transaction output observers track if output has been spent
pub trait TransactionOutputObserver: Send + Sync {
	/// Returns true if we know that output has been spent
	fn is_spent(&self, prevout: &OutPoint) -> bool;
}

/// Transaction meta provider stores transaction meta information
pub trait TransactionMetaProvider: Send + Sync {
	/// Returns None if transactin with given hash does not exist
	/// Otherwise returns transaction meta object
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}
