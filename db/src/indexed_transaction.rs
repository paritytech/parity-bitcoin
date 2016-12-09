use std::cmp;
use primitives::hash::H256;
use chain::{Transaction, OutPoint, TransactionOutput};
use PreviousTransactionOutputProvider;

#[derive(Debug, Clone)]
pub struct IndexedTransaction {
	pub hash: H256,
	pub raw: Transaction,
}

impl From<Transaction> for IndexedTransaction {
	fn from(tx: Transaction) -> Self {
		IndexedTransaction {
			hash: tx.hash(),
			raw: tx,
		}
	}
}

impl IndexedTransaction {
	pub fn new(hash: H256, transaction: Transaction) -> Self {
		IndexedTransaction {
			hash: hash,
			raw: transaction,
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
			.map(|tx| tx.raw.outputs[prevout.index as usize].clone())
	}

	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		unimplemented!();
	}
}
