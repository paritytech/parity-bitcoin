use transaction_meta::TransactionMeta;
use primitives::hash::H256;
use primitives::bytes::Bytes;

pub trait TransactionMetaProvider {
	/// get transaction metadata
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}
