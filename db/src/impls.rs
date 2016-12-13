use std::borrow::Borrow;
use chain::{OutPoint, TransactionOutput, IndexedTransactionsRef, IndexedTransaction, IndexedBlock};
use transaction_provider::PreviousTransactionOutputProvider;
use transaction_meta_provider::TransactionOutputObserver;

impl<'a, T> PreviousTransactionOutputProvider for IndexedTransactionsRef<'a, T>
	where T: Borrow<IndexedTransaction> + Send + Sync {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.transactions.iter()
			.map(Borrow::borrow)
			.find(|tx| tx.hash == prevout.hash)
			.and_then(|tx| tx.raw.outputs.get(prevout.index as usize))
			.cloned()
	}
}

impl PreviousTransactionOutputProvider for IndexedBlock {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		let txs = IndexedTransactionsRef::new(&self.transactions);
		txs.previous_transaction_output(prevout)
	}
}

impl TransactionOutputObserver for IndexedBlock {
	fn is_spent(&self, _prevout: &OutPoint) -> Option<bool> {
		// the code below is valid, but commented out due it's poor performance
		// we could optimize it by indexing all outputs once
		// let tx: IndexedTransaction = { .. }
		// let indexed_outputs: IndexedOutputs = tx.indexed_outputs();
		// indexed_outputs.is_spent()
		None

		// if previous transaction output appears more than once than we can safely
		// tell that it's spent (double spent)

		//let spends = self.transactions.iter()
			//.flat_map(|tx| &tx.raw.inputs)
			//.filter(|input| &input.previous_output == prevout)
			//.take(2)
			//.count();

		//match spends {
			//0 => None,
			//1 => Some(false),
			//2 => Some(true),
			//_ => unreachable!("spends <= 2; self.take(2); qed"),
		//}
	}
}
