use primitives::hash::H256;
use primitives::bytes::Bytes;
use chain;

pub trait TransactionProvider {

	/// returns true if store contains given transaction
	fn contains_transaction(&self, hash: &H256) -> bool {
		self.transaction(hash).is_some()
	}

	/// resolves transaction body bytes by transaction hash
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// resolves serialized transaction info by transaction hash
	fn transaction(&self, hash: &H256) -> Option<chain::Transaction>;

}
