use std::cmp;
use primitives::hash::H256;
use chain::Transaction;

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
