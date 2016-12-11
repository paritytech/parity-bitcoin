//! Some transaction validation rules,
//! require sophisticated (in more than one source) previous transaction lookups

use primitives::hash::H256;
use chain::{OutPoint, TransactionOutput};
use db::{PreviousTransactionOutputProvider, TransactionMetaProvider, TransactionMeta};

#[derive(Clone, Copy)]
pub struct DuplexTransactionOutputProvider<'a> {
	first: &'a PreviousTransactionOutputProvider,
	second: &'a PreviousTransactionOutputProvider,
}

impl<'a> DuplexTransactionOutputProvider<'a> {
	pub fn new(first: &'a PreviousTransactionOutputProvider, second: &'a PreviousTransactionOutputProvider) -> Self {
		DuplexTransactionOutputProvider {
			first: first,
			second: second,
		}
	}
}

impl<'a> PreviousTransactionOutputProvider for DuplexTransactionOutputProvider<'a> {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.first.previous_transaction_output(prevout)
			.or_else(|| self.second.previous_transaction_output(prevout))
	}
}

#[derive(Clone, Copy)]
pub struct DuplexTransactionMetaProvider<'a> {
	first: &'a TransactionMetaProvider,
	second: &'a TransactionMetaProvider,
}

impl<'a> DuplexTransactionMetaProvider<'a> {
	pub fn new(first: &'a TransactionMetaProvider, second: &'a TransactionMetaProvider) -> Self {
		DuplexTransactionMetaProvider {
			first: first,
			second: second,
		}
	}
}

impl<'a> TransactionMetaProvider for DuplexTransactionMetaProvider<'a> {
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		self.first.transaction_meta(hash)
			.or_else(|| self.second.transaction_meta(hash))
	}
}

