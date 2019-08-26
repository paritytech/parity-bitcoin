//! Some transaction validation rules,
//! require sophisticated (in more than one source) previous transaction lookups

use chain::{OutPoint, TransactionOutput};
use network::TransactionOrdering;
use TransactionOutputProvider;

#[derive(Clone, Copy)]
pub struct DuplexTransactionOutputProvider<'a> {
	first: &'a dyn TransactionOutputProvider,
	second: &'a dyn TransactionOutputProvider,
}

impl<'a> DuplexTransactionOutputProvider<'a> {
	pub fn new(first: &'a dyn TransactionOutputProvider, second: &'a dyn TransactionOutputProvider) -> Self {
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

/// Converts actual transaction index into transaction index to use in
/// TransactionOutputProvider::transaction_output call.
/// When topological ordering is used, we expect ascendant transaction (TX1)
/// to come BEFORE descendant transaction (TX2) in the block, like this:
/// [ ... TX1 ... TX2 ... ]
/// When canonical ordering is used, transactions order within block is not
/// relevant for this check and ascendant transaction (TX1) can come AFTER
/// descendant, like this:
/// [ ... TX2 ... TX1 ... ]
pub fn transaction_index_for_output_check(ordering: TransactionOrdering, tx_idx: usize) -> usize {
	match ordering {
		TransactionOrdering::Topological => tx_idx,
		TransactionOrdering::Canonical => ::std::usize::MAX,
	}
}
