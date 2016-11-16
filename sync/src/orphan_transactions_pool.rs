use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use linked_hash_map::LinkedHashMap;
use time;
use chain::Transaction;
use primitives::hash::H256;

#[derive(Debug)]
/// Storage for transactions, for which we have no parent transactions yet.
/// Transactions from this storage are either moved to verification queue, or removed at all.
pub struct OrphanTransactionsPool {
	/// Orphan transactions by hash.
	by_hash: LinkedHashMap<H256, OrphanTransaction>,
	/// Orphan transactions by parent' transaction hash
	by_parent: HashMap<H256, HashSet<H256>>,
}

#[derive(Debug)]
/// Orphan transaction representation.
pub struct OrphanTransaction {
	/// Time when this transaction was inserted to the pool
	pub insertion_time: f64,
	/// Transaction itself
	pub transaction: Transaction,
	/// Parent transactions, which are still unknown to us
	pub unknown_parents: HashSet<H256>,
}

impl OrphanTransactionsPool {
	/// Create new pool
	pub fn new() -> Self {
		OrphanTransactionsPool {
			by_hash: LinkedHashMap::new(),
			by_parent: HashMap::new(),
		}
	}

	#[cfg(test)]
	/// Get total number of transactions in pool
	pub fn len(&self) -> usize {
		self.by_hash.len()
	}

	/// Get unknown transactions in the insertion order
	pub fn transactions<'a>(&'a self) -> &'a LinkedHashMap<H256, OrphanTransaction> {
		&self.by_hash
	}

	/// Insert orphan transaction
	pub fn insert(&mut self, hash: H256, transaction: Transaction, unknown_parents: HashSet<H256>) {
		assert!(!self.by_hash.contains_key(&hash));
		assert!(unknown_parents.iter().all(|h| transaction.inputs.iter().any(|i| &i.previous_output.hash == h)));

		for unknown_parent in unknown_parents.iter() {
			self.by_parent.entry(unknown_parent.clone())
				.or_insert_with(HashSet::new)
				.insert(hash.clone());
		}
		self.by_hash.insert(hash, OrphanTransaction::new(transaction, unknown_parents));
	}

	/// Remove all transactions, depending on this parent
	pub fn remove_transactions_for_parent(&mut self, hash: &H256) -> Vec<(H256, Transaction)> {
		assert!(!self.by_hash.contains_key(hash));

		// remove direct children of hash
		let mut removed_orphans_hashes: Vec<H256> = Vec::new();
		let mut removed_orphans: Vec<(H256, Transaction)> = Vec::new();
		if let Entry::Occupied(children_entry) = self.by_parent.entry(hash.clone()) {
			for child in children_entry.get() {
				if {
					let child_entry = self.by_hash.get_mut(child).expect("every entry in by_parent.values() has corresponding entry in by_hash.keys()");
					child_entry.remove_known_parent(hash)
				} {
					removed_orphans_hashes.push(child.clone());
					removed_orphans.push((child.clone(), self.by_hash.remove(child).expect("checked couple of lines above").transaction));
				};
			}

			children_entry.remove_entry();
		}

		// then also remove grandchildren of hash & so on
		for child_hash in removed_orphans_hashes {
			removed_orphans.extend(self.remove_transactions_for_parent(&child_hash));
		}

		removed_orphans
	}

	/// Remove transactions with given hashes + all dependent blocks
	pub fn remove_transactions(&mut self, hashes: &[H256]) -> Vec<(H256, Transaction)> {
		let mut removed: Vec<(H256, Transaction)> = Vec::new();
		for hash in hashes {
			if let Some(transaction) = self.by_hash.remove(hash) {
				removed.push((hash.clone(), transaction.transaction));
			}
			removed.extend(self.remove_transactions_for_parent(hash));
		}
		removed
	}
}

impl OrphanTransaction {
	/// Create new orphaned transaction
	pub fn new(transaction: Transaction, unknown_parents: HashSet<H256>) -> Self {
		OrphanTransaction {
			insertion_time: time::precise_time_s(),
			transaction: transaction,
			unknown_parents: unknown_parents,
		}
	}

	/// Remove parent, which is now known. Return true if all parents all now known
	pub fn remove_known_parent(&mut self, parent_hash: &H256) -> bool {
		self.unknown_parents.remove(parent_hash);
		self.unknown_parents.is_empty()
	}
}

#[cfg(test)]
mod tests {
	use std::collections::HashSet;
	use test_data::{TransactionBuilder, ChainBuilder};
	use primitives::hash::H256;
	use super::OrphanTransactionsPool;

	#[test]
	fn orphan_transaction_pool_empty_on_start() {
		let pool = OrphanTransactionsPool::new();
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn orphan_transaction_pool_insert_dependent_transactions() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(100).store(chain)			// t1
			.into_input(0).add_output(200).store(chain)				// t1 -> t2
			.into_input(0).add_output(300).store(chain)				// t1 -> t2 -> t3
			.set_default_input(0).set_output(400).store(chain)		// t4
			.into_input(0).set_output(500).store(chain);			// t4 -> t5
		let t2_unknown: HashSet<H256> = chain.at(1).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let t3_unknown: HashSet<H256> = chain.at(2).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let t5_unknown: HashSet<H256> = chain.at(4).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();

		let mut pool = OrphanTransactionsPool::new();
		pool.insert(chain.at(1).hash(), chain.at(1), t2_unknown); // t2
		pool.insert(chain.at(2).hash(), chain.at(2), t3_unknown); // t3
		pool.insert(chain.at(4).hash(), chain.at(4), t5_unknown); // t5
		assert_eq!(pool.len(), 3);

		let removed = pool.remove_transactions_for_parent(&chain.at(0).hash());
		assert_eq!(pool.len(), 1);
		let removed: Vec<H256> = removed.into_iter().map(|(h, _)| h).collect();
		assert_eq!(removed, vec![chain.at(1).hash(), chain.at(2).hash()]);

		let removed = pool.remove_transactions_for_parent(&chain.at(3).hash());
		assert_eq!(pool.len(), 0);
		let removed: Vec<H256> = removed.into_iter().map(|(h, _)| h).collect();
		assert_eq!(removed, vec![chain.at(4).hash()]);
	}

	#[test]
	fn orphan_transaction_pool_remove_transactions() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(100).store(chain)			// t1
			.into_input(0).add_output(200).store(chain)				// t1 -> t2
			.into_input(0).add_output(300).store(chain)				// t1 -> t2 -> t3
			.set_default_input(0).set_output(400).store(chain)		// t4
			.into_input(0).set_output(500).store(chain)				// t4 -> t5
			.set_default_input(0).set_output(600).store(chain)		// t6
			.into_input(0).set_output(700).store(chain);			// t6 -> t7
		let t2_unknown: HashSet<H256> = chain.at(1).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let t3_unknown: HashSet<H256> = chain.at(2).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let t5_unknown: HashSet<H256> = chain.at(4).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();
		let t7_unknown: HashSet<H256> = chain.at(6).inputs.iter().map(|i| i.previous_output.hash.clone()).collect();

		let mut pool = OrphanTransactionsPool::new();
		pool.insert(chain.at(1).hash(), chain.at(1), t2_unknown); // t2
		pool.insert(chain.at(2).hash(), chain.at(2), t3_unknown); // t3
		pool.insert(chain.at(4).hash(), chain.at(4), t5_unknown); // t5
		pool.insert(chain.at(6).hash(), chain.at(6), t7_unknown); // t7
		assert_eq!(pool.len(), 4);

		let removed = pool.remove_transactions(&vec![chain.at(1).hash(), chain.at(3).hash()]);
		assert_eq!(pool.len(), 1);
		let removed: Vec<H256> = removed.into_iter().map(|(h, _)| h).collect();
		assert_eq!(removed, vec![chain.at(1).hash(), chain.at(2).hash(), chain.at(4).hash()]);

		let removed = pool.remove_transactions(&vec![chain.at(6).hash()]);
		assert_eq!(pool.len(), 0);
		let removed: Vec<H256> = removed.into_iter().map(|(h, _)| h).collect();
		assert_eq!(removed, vec![chain.at(6).hash()]);
	}
}
