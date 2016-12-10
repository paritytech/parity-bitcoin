use primitives::hash::H256;
use chain::OutPoint;
use transaction_meta::TransactionMeta;

pub trait TransactionOutputObserver {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool>;
}

pub trait TransactionMetaProvider {
	/// get transaction metadata
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}
