use primitives::hash::H256;
use chain::OutPoint;
use transaction_meta::TransactionMeta;

pub trait TransactionMetaProvider {
	/// get transaction metadata
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;

	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		self.transaction_meta(&prevout.hash)
			.and_then(|meta| meta.is_spent(prevout.index as usize))
	}
}
