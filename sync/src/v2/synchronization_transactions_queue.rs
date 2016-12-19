use std::collections::{HashSet, VecDeque};
use linked_hash_map::LinkedHashMap;
use chain::IndexedTransaction;
use primitives::hash::H256;
use types::{MemoryPoolRef, StorageRef};
use utils::OrphanTransactionsPool;

/// Transactions synchronization state
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum TransactionState {
	/// Transaction is unknown
	Unknown,
	/// Orphan
	Orphan,
	/// Currently verifying
	Verifying,
	/// In memory pool
	InMemory,
	/// In storage
	Stored,
}

/// Synchronization chain information
pub struct Information {
	/// Number of orphaned transactions
	pub orphaned: usize,
	/// Number of transactions currently verifying
	pub verifying: usize,
	/// Number of transactions currently resided in memory pool
	pub in_memory: usize,
}

/// Blockchain transactions from synchroniation point of view, consisting of:
/// 1) all transactions from the `storage` [oldest transactions]
/// 2) all transactions currently residing in memory pool
/// 3) all transactions currently verifying by `synchronization_verifier`
pub trait TransactionsQueue {
	/// Returns queue information
	fn information(&self) -> Information;
	/// Returns state of transaction with given hash
	fn transaction_state(&self, hash: &H256) -> TransactionState;
	/// Insert orphan transaction
	fn insert_orphan_transaction(&mut self, transaction: IndexedTransaction, unknown_inputs: HashSet<H256>);
	/// Remove orphan transactions, which depends on given transaction.
	fn remove_orphan_children(&mut self, hash: &H256) -> Vec<IndexedTransaction>;
	/// Insert verifying transaction.
	fn insert_verifying_transaction(&mut self, transaction: IndexedTransaction);
	/// Forget verifying transaction.
	fn forget_verifying_transaction(&mut self, hash: &H256) -> bool;
	/// Forget verifying transaction and all verifying children.
	fn forget_verifying_transaction_with_children(&mut self, hash: &H256);
	/// Insert verified transaction to memory pool.
	fn insert_verified_transaction(&mut self, transaction: IndexedTransaction);
}

/// Transactions queue implementation
pub struct TransactionsQueueImpl {
	/// Orphaned transactions
	orphan_pool: OrphanTransactionsPool,
	/// Currently verifying transactions
	verifying: LinkedHashMap<H256, IndexedTransaction>,
	/// Transactions memory pool
	memory_pool: MemoryPoolRef,
	/// Storage reference
	storage: StorageRef,
}

impl TransactionsQueueImpl {
	pub fn new(memory_pool: MemoryPoolRef, storage: StorageRef) -> Self {
		TransactionsQueueImpl {
			orphan_pool: OrphanTransactionsPool::new(),
			verifying: LinkedHashMap::new(),
			memory_pool: memory_pool,
			storage: storage,
		}
	}
}

impl TransactionsQueue for TransactionsQueueImpl {
	fn information(&self) -> Information {
		Information {
			orphaned: self.orphan_pool.len(),
			verifying: self.verifying.len(),
			in_memory: self.memory_pool.read().information().transactions_count,
		}
	}

	fn transaction_state(&self, hash: &H256) -> TransactionState {
		if self.orphan_pool.contains(hash) {
			return TransactionState::Orphan;
		}
		if self.verifying.contans_key(hash) {
			return TransactionState::Verifying;
		}
		if self.storage.contains_transaction(hash) {
			return TransactionState::Stored;
		}
		if self.memory_pool.read().contains(hash) {
			return TransactionState::InMemory;
		}

		TransactionState::Unknown
	}

	fn insert_orphan_transaction(&mut self, transaction: IndexedTransaction, unknown_inputs: HashSet<H256>) {
		self.orphan_pool.insert(transaction, unknown_inputs);
	}

	fn remove_orphan_children(&mut self, hash: &H256) -> Vec<IndexedTransaction> {
		self.orphan_pool.remove_transactions_for_parent(hash)
	}

	fn insert_verifying_transaction(&mut self, transaction: IndexedTransaction) {
		self.verifying.insert(transaction.hash.clone(), transaction);
	}

	fn forget_verifying_transaction(&mut self, hash: &H256) -> bool {
		self.verifying.remove(hash).is_some()
	}

	fn forget_verifying_transaction_with_children(&mut self, hash: &H256) {
		self.forget_verifying_transaction(hash);

		// TODO: suboptimal
		let mut queue: VecDeque<H256> = VecDeque::new();
		queue.push_back(hash.clone());
		while let Some(hash) = queue.pop_front() {
			let all_keys: Vec<_> = self.verifying_transactions.keys().cloned().collect();
			for h in all_keys {
				let remove_verifying_transaction = {
					if let Some(entry) = self.verifying.get(&h) {
						if entry.inputs.iter().any(|i| i.previous_output.hash == hash) {
							queue.push_back(h.clone());
							true
						} else {
							false
						}
					} else {
						// iterating by previously read keys
						unreachable!()
					}
				};

				if remove_verifying_transaction {
					self.verifying.remove(&h);
				}
			}
		}
	}

	fn insert_verified_transaction(&mut self, transaction: IndexedTransaction) {
		// we have verified transaction, but possibly this transaction replaces
		// existing transaction from memory pool
		// => remove previous transactions before
		let memory_pool = self.memory_pool.write();
		for input in &transaction.raw.inputs {
			memory_pool.remove_by_prevout(&input.previous_output);
		}

		// now insert transaction itself
		memory_pool.insert_verified(transaction);
	}
}
