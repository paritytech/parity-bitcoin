//! Some transaction validation rules,
//! require sophisticated (in more than one source) previous transaction lookups

use chain::{OutPoint, TransactionOutput};
use db::{PreviousTransactionOutputProvider, TransactionOutputObserver};

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
	fn previous_transaction_output(&self, prevout: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		self.first.previous_transaction_output(prevout, transaction_index)
			.or_else(|| self.second.previous_transaction_output(prevout, transaction_index))
	}
}

#[derive(Clone, Copy)]
pub struct DuplexTransactionOutputObserver<'a> {
	first: &'a TransactionOutputObserver,
	second: &'a TransactionOutputObserver,
}

impl<'a> DuplexTransactionOutputObserver<'a> {
	pub fn new(first: &'a TransactionOutputObserver, second: &'a TransactionOutputObserver) -> Self {
		DuplexTransactionOutputObserver {
			first: first,
			second: second,
		}
	}
}

impl<'a> TransactionOutputObserver for DuplexTransactionOutputObserver<'a> {
	fn is_spent(&self, prevout: &OutPoint) -> bool {
		self.first.is_spent(prevout) || self.second.is_spent(prevout)
	}
}

pub struct NoopStore;

impl PreviousTransactionOutputProvider for NoopStore {
	fn previous_transaction_output(&self, _prevout: &OutPoint, _transaction_index: usize) -> Option<TransactionOutput> {
		None
	}
}

impl TransactionOutputObserver for NoopStore {
	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		false
	}
}
