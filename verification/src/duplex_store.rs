//! Some transaction validation rules,
//! require sophisticated (in more than one source) previous transaction lookups

use chain::{OutPoint, TransactionOutput};
use storage::TransactionOutputProvider;

#[derive(Clone, Copy)]
pub struct DuplexTransactionOutputProvider<'a> {
	first: &'a TransactionOutputProvider,
	second: &'a TransactionOutputProvider,
}

impl<'a> DuplexTransactionOutputProvider<'a> {
	pub fn new(first: &'a TransactionOutputProvider, second: &'a TransactionOutputProvider) -> Self {
		DuplexTransactionOutputProvider {
			first: first,
			second: second,
		}
	}
}

impl<'a> TransactionOutputProvider for DuplexTransactionOutputProvider<'a> {
	fn transaction_output(&self, prevout: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		self.first.transaction_output(prevout, transaction_index)
			.or_else(|| self.second.transaction_output(prevout, transaction_index))
	}

	fn is_spent(&self, prevout: &OutPoint) -> bool {
		self.first.is_spent(prevout) || self.second.is_spent(prevout)
	}
}

pub struct NoopStore;

impl TransactionOutputProvider for NoopStore {
	fn transaction_output(&self, _prevout: &OutPoint, _transaction_index: usize) -> Option<TransactionOutput> {
		None
	}

	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		false
	}
}
