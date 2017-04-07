use chain::{OutPoint, TransactionOutput, IndexedBlock, IndexedTransaction};
use transaction_provider::PreviousTransactionOutputProvider;
use transaction_meta_provider::TransactionOutputObserver;

fn transaction_output(transactions: &[IndexedTransaction], prevout: &OutPoint) -> Option<TransactionOutput> {
	transactions.iter()
		.find(|tx| tx.hash == prevout.hash)
		.and_then(|tx| tx.raw.outputs.get(prevout.index as usize))
		.cloned()
}

fn is_spent(transactions: &[IndexedTransaction], prevout: &OutPoint) -> bool {
	// the code below is valid, but has rather poor performance

	// if previous transaction output appears more than once than we can safely
	// tell that it's spent (double spent)
	let spends = transactions.iter()
		.flat_map(|tx| &tx.raw.inputs)
		.filter(|input| &input.previous_output == prevout)
		.take(2)
		.count();

	spends == 2
}

impl PreviousTransactionOutputProvider for IndexedBlock {
	fn previous_transaction_output(&self, prevout: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		transaction_output(&self.transactions[..transaction_index], prevout)
	}
}

impl TransactionOutputObserver for IndexedBlock {
	fn is_spent(&self, prevout: &OutPoint) -> bool {
		is_spent(&self.transactions, prevout)
	}
}
