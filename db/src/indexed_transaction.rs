use std::cmp;
use primitives::hash::H256;
use chain::{Transaction, OutPoint, TransactionOutput};
use PreviousTransactionOutputProvider;

#[derive(Debug)]
pub struct IndexedTransaction {
	pub transaction: Transaction,
	pub hash: H256,
}

impl From<Transaction> for IndexedTransaction {
	fn from(t: Transaction) -> Self {
		let hash = t.hash();
		IndexedTransaction {
			transaction: t,
			hash: hash,
		}
	}
}

impl cmp::PartialEq for IndexedTransaction {
	fn eq(&self, other: &Self) -> bool {
		self.hash == other.hash
	}
}

impl<'a> PreviousTransactionOutputProvider for &'a [IndexedTransaction] {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.iter()
			.find(|tx| tx.hash == prevout.hash)
			.map(|tx| tx.transaction.outputs[prevout.index as usize].clone())
	}

	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		unimplemented!();
	}
}
