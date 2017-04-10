use std::cmp;
use chain::{OutPoint, TransactionOutput, IndexedBlock, IndexedTransaction};
use {TransactionOutputProvider};

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

impl TransactionOutputProvider for IndexedBlock {
	fn transaction_output(&self, outpoint: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		let take = cmp::min(transaction_index, self.transactions.len());
		transaction_output(&self.transactions[..take], outpoint)
	}

	fn is_spent(&self, outpoint: &OutPoint) -> bool {
		is_spent(&self.transactions, outpoint)
	}
}
