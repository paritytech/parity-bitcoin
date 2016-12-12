//! Some transaction validation rules,
//! require sophisticated (in more than one source) previous transaction lookups

use chain::{OutPoint, TransactionOutput};
use db::PreviousTransactionOutputProvider;

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
