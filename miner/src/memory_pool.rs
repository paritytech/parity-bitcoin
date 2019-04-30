//! Transactions memory pool
//!
//! `MemoryPool` keeps track of all transactions seen by the node (received from other peers) and own transactions
//! and orders them by given strategies. It works like multi-indexed priority queue, giving option to pop 'top'
//! transactions.
//! It also guarantees that ancestor-descendant relation won't break during ordered removal (ancestors always removed
//! before descendants). Removal using `remove_by_hash` can break this rule.
use storage::{TransactionProvider, TransactionOutputProvider};
use primitives::bytes::Bytes;
use primitives::hash::H256;
use chain::{IndexedTransaction, Transaction, OutPoint, TransactionOutput};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::hash::{Hash, Hasher};
use ser::{Serializable, serialize};
use heapsize::HeapSizeOf;
use fee::MemoryPoolFeeCalculator;

/// Transactions ordering strategy
#[cfg_attr(feature="cargo-clippy", allow(enum_variant_names))]
#[derive(Debug, Clone, Copy)]
pub enum OrderingStrategy {
	/// Order transactions by the time they have entered the memory pool
	ByTimestamp,
	/// Order transactions by their individual mining score
	ByTransactionScore,
	/// Order transactions by their in-pool package mining score (score for mining this transaction + all descendants transactions)
	ByPackageScore,
}

/// Information on current `MemoryPool` state
#[derive(Debug)]
pub struct Information {
	/// Number of transactions currently in the `MemoryPool`
	pub transactions_count: usize,
	/// Total number of bytes occupied by transactions from the `MemoryPool`
	pub transactions_size_in_bytes: usize,
}

/// Transactions memory pool
#[derive(Debug)]
pub struct MemoryPool {
	/// Transactions storage
	storage: Storage,
	/// Do we accept zero fee transactions?
	accept_zero_fee_transactions: bool,
}

/// Single entry
#[derive(Debug)]
pub struct Entry {
	/// Transaction
	pub transaction: Transaction,
	/// In-pool ancestors hashes for this transaction
	pub ancestors: HashSet<H256>,
	/// Transaction hash (stored for effeciency)
	pub hash: H256,
	/// Transaction size (stored for effeciency)
	pub size: usize,
	/// Throughout index of this transaction in memory pool (non persistent)
	pub storage_index: u64,
	/// Transaction fee (stored for efficiency)
	pub miner_fee: u64,
	/// Virtual transaction fee (a way to prioritize/penalize transaction)
	pub miner_virtual_fee: i64,
	/// size + Sum(size) for all in-pool descendants
	pub package_size: usize,
	/// miner_fee + Sum(miner_fee) for all in-pool descendants
	pub package_miner_fee: u64,
	/// miner_virtual_fee + Sum(miner_virtual_fee) for all in-pool descendants
	pub package_miner_virtual_fee: i64,
}

/// Multi-index transactions storage
#[derive(Debug)]
struct Storage {
	/// Throughout transactions counter
	counter: u64,
	/// Total transactions size (when serialized) in bytes
	transactions_size_in_bytes: usize,
	/// By-hash storage
	by_hash: HashMap<H256, Entry>,
	/// Transactions by previous output
	by_previous_output: HashMap<HashedOutPoint, H256>,
	/// References storage
	references: ReferenceStorage,
}

/// Multi-index storage which holds references to entries from `Storage::by_hash`
#[derive(Debug, Clone)]
struct ReferenceStorage {
	/// By-input storage
	by_input: HashMap<H256, HashSet<H256>>,
	/// Pending entries
	pending: HashSet<H256>,
	/// Ordered storage
	ordered: OrderedReferenceStorage,
}

/// Multi-index orderings storage which holds ordered references to entries from `Storage::by_hash`
#[derive(Debug, Clone)]
struct OrderedReferenceStorage {
	/// By-entry-time storage
	by_storage_index: BTreeSet<ByTimestampOrderedEntry>,
	/// By-score storage
	by_transaction_score: BTreeSet<ByTransactionScoreOrderedEntry>,
	/// By-package-score strategy
	by_package_score: BTreeSet<ByPackageScoreOrderedEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ByTimestampOrderedEntry {
	/// Transaction hash
	hash: H256,
	/// Throughout index of this transaction in memory pool (non persistent)
	storage_index: u64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct ByTransactionScoreOrderedEntry {
	/// Transaction hash
	hash: H256,
	/// Transaction size
	size: usize,
	/// Transaction fee
	miner_fee: u64,
	/// Virtual transaction fee
	miner_virtual_fee: i64,
}

#[derive(Debug, Eq, PartialEq, Clone)]
struct ByPackageScoreOrderedEntry {
	/// Transaction hash
	hash: H256,
	/// size + Sum(size) for all in-pool descendants
	package_size: usize,
	/// miner_fee + Sum(miner_fee) for all in-pool descendants
	package_miner_fee: u64,
	/// miner_virtual_fee + Sum(miner_virtual_fee) for all in-pool descendants
	package_miner_virtual_fee: i64,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct HashedOutPoint {
	/// Transaction output point
	out_point: OutPoint,
}

/// Result of checking double spend with
#[derive(Debug, PartialEq)]
pub enum DoubleSpendCheckResult {
	/// No double spend
	NoDoubleSpend,
	/// Input {self.1, self.2} of new transaction is already spent in previous final memory-pool transaction {self.0}
	DoubleSpend(H256, H256, u32),
	/// Some inputs of new transaction are already spent by non-final memory-pool transactions
	NonFinalDoubleSpend(NonFinalDoubleSpendSet),
}

/// Set of transaction outputs, which can be replaced if newer transaction
/// replaces non-final transaction in memory pool
#[derive(Debug, PartialEq)]
pub struct NonFinalDoubleSpendSet {
	/// Double-spend outputs (outputs of newer transaction, which are also spent by nonfinal transactions of mempool)
	pub double_spends: HashSet<HashedOutPoint>,
	/// Outputs which also will be removed from memory pool in case of newer transaction insertion
	/// (i.e. outputs of nonfinal transactions && their descendants)
	pub dependent_spends: HashSet<HashedOutPoint>,
}

impl From<OutPoint> for HashedOutPoint {
	fn from(out_point: OutPoint) -> Self {
		HashedOutPoint {
			out_point: out_point,
		}
	}
}

impl Hash for HashedOutPoint {
	fn hash<H>(&self, state: &mut H) where H: Hasher {
		state.write(&serialize(&self.out_point));
		state.finish();
	}
}

impl<'a> From<&'a Entry> for ByTimestampOrderedEntry {
	fn from(entry: &'a Entry) -> Self {
		ByTimestampOrderedEntry {
			hash: entry.hash.clone(),
			storage_index: entry.storage_index,
		}
	}
}

impl<'a> From<&'a Entry> for ByTransactionScoreOrderedEntry {
	fn from(entry: &'a Entry) -> Self {
		ByTransactionScoreOrderedEntry {
			hash: entry.hash.clone(),
			size: entry.size,
			miner_fee: entry.miner_fee,
			miner_virtual_fee: entry.miner_virtual_fee,
		}
	}
}

impl<'a> From<&'a Entry> for ByPackageScoreOrderedEntry {
	fn from(entry: &'a Entry) -> Self {
		ByPackageScoreOrderedEntry {
			hash: entry.hash.clone(),
			package_size: entry.package_size,
			package_miner_fee: entry.package_miner_fee,
			package_miner_virtual_fee: entry.package_miner_virtual_fee,
		}
	}
}

impl PartialOrd for ByTimestampOrderedEntry {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ByTimestampOrderedEntry {
	fn cmp(&self, other: &Self) -> Ordering {
		let order = self.storage_index.cmp(&other.storage_index);
		if order != Ordering::Equal {
			return order
		}

		self.hash.cmp(&other.hash)
	}
}

impl PartialOrd for ByTransactionScoreOrderedEntry {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ByTransactionScoreOrderedEntry {
	fn cmp(&self, other: &Self) -> Ordering {
		// lesser miner score means later removal
		let left = (self.miner_fee as i64 + self.miner_virtual_fee) * (other.size as i64);
		let right = (other.miner_fee as i64 + other.miner_virtual_fee) * (self.size as i64);
		let order = right.cmp(&left);
		if order != Ordering::Equal {
			return order
		}

		self.hash.cmp(&other.hash)
	}
}

impl PartialOrd for ByPackageScoreOrderedEntry {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

impl Ord for ByPackageScoreOrderedEntry {
	fn cmp(&self, other: &Self) -> Ordering {
		// lesser miner score means later removal
		let left = (self.package_miner_fee as i64 + self.package_miner_virtual_fee) * (other.package_size as i64);
		let right = (other.package_miner_fee as i64 + other.package_miner_virtual_fee) * (self.package_size as i64);
		let order = right.cmp(&left);
		if order != Ordering::Equal {
			return order
		}

		self.hash.cmp(&other.hash)
	}
}

impl HeapSizeOf for Entry {
	fn heap_size_of_children(&self) -> usize {
		self.transaction.heap_size_of_children() + self.ancestors.heap_size_of_children()
	}
}

impl Storage {
	pub fn new() -> Self {
		Storage {
			counter: 0,
			transactions_size_in_bytes: 0,
			by_hash: HashMap::new(),
			by_previous_output: HashMap::new(),
			references: ReferenceStorage {
				by_input: HashMap::new(),
				pending: HashSet::new(),
				ordered: OrderedReferenceStorage {
					by_storage_index: BTreeSet::new(),
					by_transaction_score: BTreeSet::new(),
					by_package_score: BTreeSet::new(),
				},
			},
		}
	}

	pub fn insert(&mut self, entry: Entry) {
		// update pool information
		self.transactions_size_in_bytes += entry.size;

		// remember that this transactions depends on its inputs
		for input_hash in entry.transaction.inputs.iter().map(|input| &input.previous_output.hash) {
			self.references.by_input.entry(input_hash.clone()).or_insert_with(HashSet::new).insert(entry.hash.clone());
		}

		// update score of all packages this transaction is in
		for ancestor_hash in &entry.ancestors {
			if let Some(ancestor_entry) = self.by_hash.get_mut(ancestor_hash) {
				let removed = self.references.ordered.by_package_score.remove(&(ancestor_entry as &Entry).into());

				ancestor_entry.package_size += entry.size;
				ancestor_entry.package_miner_fee += entry.package_miner_fee;
				ancestor_entry.package_miner_virtual_fee += entry.package_miner_virtual_fee;

				if removed {
					self.references.ordered.by_package_score.insert((ancestor_entry as &Entry).into());
				}
			}
		}

		// insert either to pending queue or to orderings
		if self.references.has_in_pool_ancestors(None, &self.by_hash, &entry.transaction) {
			self.references.pending.insert(entry.hash.clone());
		}
		else {
			self.references.ordered.insert_to_orderings(&entry);
		}

		// remember that all inputs of this transaction are spent
		for input in &entry.transaction.inputs {
			let previous_tx = self.by_previous_output.insert(input.previous_output.clone().into(), entry.hash.clone());
			assert_eq!(previous_tx, None); // transaction must be verified before => no double spend
		}

		// add to by_hash storage
		self.by_hash.insert(entry.hash.clone(), entry);
	}

	pub fn get_by_hash(&self, h: &H256) -> Option<&Entry> {
		self.by_hash.get(h)
	}

	pub fn contains(&self, hash: &H256) -> bool {
		self.by_hash.contains_key(hash)
	}

	pub fn is_output_spent(&self, prevout: &OutPoint) -> bool {
		self.by_previous_output.contains_key(&prevout.clone().into())
	}

	pub fn set_virtual_fee(&mut self, h: &H256, virtual_fee: i64) {
		// for updating ancestors
		let mut miner_virtual_fee_change = 0i64;
		let mut ancestors: Option<Vec<H256>> = None;

		// modify the entry itself
		if let Some(entry) = self.by_hash.get_mut(h) {
			let insert_to_package_score = self.references.ordered.by_package_score.remove(&(entry as &Entry).into());
			let insert_to_transaction_score = self.references.ordered.by_transaction_score.remove(&(entry as &Entry).into());

			miner_virtual_fee_change = virtual_fee - entry.miner_virtual_fee;
			if !entry.ancestors.is_empty() {
				ancestors = Some(entry.ancestors.iter().cloned().collect());
			}

			entry.miner_virtual_fee = virtual_fee;

			if insert_to_transaction_score {
				self.references.ordered.by_transaction_score.insert((entry as &Entry).into());
			}
			if insert_to_package_score {
				self.references.ordered.by_package_score.insert((entry as &Entry).into());
			}
		}

		// now modify all ancestor entries
		if miner_virtual_fee_change != 0 {
			ancestors.map(|ancestors| {
				for ancestor_hash in ancestors {
					if let Some(ancestor_entry) = self.by_hash.get_mut(&ancestor_hash) {
						let insert_to_package_score = self.references.ordered.by_package_score.remove(&(ancestor_entry as &Entry).into());
						ancestor_entry.package_miner_virtual_fee += miner_virtual_fee_change;
						if insert_to_package_score {
							self.references.ordered.by_package_score.insert((ancestor_entry as &Entry).into());
						}
					}
				}
			});
		}
	}

	pub fn read_by_hash(&self, h: &H256) -> Option<&Transaction> {
		self.by_hash.get(h).map(|e| &e.transaction)
	}

	pub fn read_with_strategy(&self, strategy: OrderingStrategy) -> Option<H256> {
		match strategy {
			OrderingStrategy::ByTimestamp => self.references.ordered.by_storage_index.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByTransactionScore => self.references.ordered.by_transaction_score.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByPackageScore => self.references.ordered.by_package_score.iter().map(|entry| entry.hash.clone()).nth(0),
		}
	}

	pub fn remove_by_hash(&mut self, h: &H256) -> Option<Entry> {
		self.by_hash.remove(h)
			.map(|entry| {
				// update pool information
				self.transactions_size_in_bytes -= entry.size;

				// forget that all inputs of this transaction are spent
				for input in &entry.transaction.inputs {
					let spent_in_tx = self.by_previous_output.remove(&input.previous_output.clone().into())
						.expect("by_spent_output is filled for each incoming transaction inputs; so the drained value should exist; qed");
					assert_eq!(&spent_in_tx, h);
				}

				// remove from storage
				self.references.remove(None, &self.by_hash, &entry);

				entry
			})
	}

	pub fn check_double_spend(&self, transaction: &Transaction) -> DoubleSpendCheckResult {
		let mut double_spends: HashSet<HashedOutPoint> = HashSet::new();
		let mut dependent_spends: HashSet<HashedOutPoint> = HashSet::new();

		for input in &transaction.inputs {
			// find transaction that spends the same output
			let prevout: HashedOutPoint = input.previous_output.clone().into();
			if let Some(entry_hash) = self.by_previous_output.get(&prevout).cloned() {
				// check if this is final transaction. If so, that's a potential double-spend error
				let entry = self.by_hash.get(&entry_hash).expect("checked that it exists line above; qed");
				if entry.transaction.is_final() {
					return DoubleSpendCheckResult::DoubleSpend(entry_hash,	 prevout.out_point.hash, prevout.out_point.index);
				}
				// else remember this double spend
				double_spends.insert(prevout.clone());
				// and 'virtually' remove entry && all descendants from mempool
				let mut queue: VecDeque<HashedOutPoint> = VecDeque::new();
				queue.push_back(prevout);
				while let Some(dependent_prevout) = queue.pop_front() {
					// if the same output is already spent with another in-pool transaction
					if let Some(dependent_entry_hash) = self.by_previous_output.get(&dependent_prevout).cloned() {
						let dependent_entry = self.by_hash.get(&dependent_entry_hash).expect("checked that it exists line above; qed");
						let dependent_outputs: Vec<_> = dependent_entry.transaction.outputs.iter().enumerate().map(|(idx, _)| OutPoint {
							hash: dependent_entry_hash.clone(),
							index: idx as u32,
						}.into()).collect();
						dependent_spends.extend(dependent_outputs.clone());
						queue.extend(dependent_outputs);
					}
				}
			}
		}

		if double_spends.is_empty() {
			DoubleSpendCheckResult::NoDoubleSpend
		} else {
			DoubleSpendCheckResult::NonFinalDoubleSpend(NonFinalDoubleSpendSet {
				double_spends: double_spends,
				dependent_spends: dependent_spends,
			})
		}
	}

	pub fn remove_by_prevout(&mut self, prevout: &OutPoint) -> Option<Vec<IndexedTransaction>> {
		let mut queue: VecDeque<OutPoint> = VecDeque::new();
		let mut removed: Vec<IndexedTransaction> = Vec::new();
		queue.push_back(prevout.clone());

		while let Some(prevout) = queue.pop_front() {
			if let Some(entry_hash) = self.by_previous_output.get(&prevout.clone().into()).cloned() {
				let entry = self.remove_by_hash(&entry_hash).expect("checked that it exists line above; qed");
				queue.extend(entry.transaction.outputs.iter().enumerate().map(|(idx, _)| OutPoint {
					hash: entry_hash.clone(),
					index: idx as u32,
				}));
				removed.push(IndexedTransaction::new(entry.hash, entry.transaction));
			}
		}

		Some(removed)
	}

	pub fn remove_by_parent_hash(&mut self, h: &H256) -> Option<Vec<IndexedTransaction>> {
		// this code will run only when ancestor transaction is inserted
		// in memory pool after its descendants
		if let Some(mut descendants) = self.references.by_input.get(h).map(|d| d.iter().cloned().collect::<Vec<H256>>()) {
			// prepare Vec of all descendants hashes
			let mut all_descendants: HashSet<H256> = HashSet::new();
			while let Some(descendant) = descendants.pop() {
				if all_descendants.contains(&descendant) {
					continue
				}
				all_descendants.insert(descendant.clone());

				if let Some(grand_descendants) = self.references.by_input.get(&descendant) {
					descendants.extend(grand_descendants.iter().cloned());
				}
			}

			// topologically sort descendants
			let mut all_descendants: Vec<_> = all_descendants.iter().collect();
			all_descendants.sort_by(|left, right| {
				let left = self.by_hash.get(left)
					.expect("`left` is read from `by_input`; all entries from `by_input` have corresponding entries in `by_hash`; qed");
				let right = self.by_hash.get(right)
					.expect("`right` is read from `by_input`; all entries from `by_input` have corresponding entries in `by_hash`; qed");
				if left.ancestors.contains(&right.hash) {
					return Ordering::Greater;
				}
				if right.ancestors.contains(&left.hash) {
					return Ordering::Less;
				}
				Ordering::Equal
			});

			// move all descendants out of storage for later insertion
			Some(all_descendants.into_iter()
					.filter_map(|hash| self.remove_by_hash(hash).map(|entry| IndexedTransaction::new(entry.hash, entry.transaction)))
					.collect())
		}
		else {
			None
		}
	}

	pub fn remove_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<IndexedTransaction> {
		let top_hash = match strategy {
			OrderingStrategy::ByTimestamp => self.references.ordered.by_storage_index.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByTransactionScore => self.references.ordered.by_transaction_score.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByPackageScore => self.references.ordered.by_package_score.iter().map(|entry| entry.hash.clone()).nth(0),
		};
		top_hash.map(|hash| {
			let entry = self.remove_by_hash(&hash)
				.expect("`hash` is read from `references`; entries in `references` have corresponging entries in `by_hash`; `remove_by_hash` removes entry from `by_hash`; qed");
			IndexedTransaction::new(entry.hash, entry.transaction)
		})
	}

	pub fn remove_n_with_strategy(&mut self, mut n: usize, strategy: OrderingStrategy) -> Vec<IndexedTransaction> {
		let mut result: Vec<IndexedTransaction> = Vec::new();
		loop {
			if n == 0 {
				break;
			}
			n -= 1;

			result.push(match self.remove_with_strategy(strategy) {
				Some(transaction) => transaction,
				None => break,
			})
		}
		result
	}

	pub fn get_transactions_ids(&self) -> Vec<H256> {
		self.by_hash.keys().cloned().collect()
	}
}

impl ReferenceStorage {
	pub fn has_in_pool_ancestors(&self, removed: Option<&HashSet<H256>>, by_hash: &HashMap<H256, Entry>, transaction: &Transaction) -> bool {
		transaction.inputs.iter()
			.any(|input| by_hash.contains_key(&input.previous_output.hash)
				&& !removed.map_or(false, |r| r.contains(&input.previous_output.hash)))
	}

	pub fn remove(&mut self, removed: Option<&HashSet<H256>>, by_hash: &HashMap<H256, Entry>, entry: &Entry) {
		// for each pending descendant transaction
		if let Some(descendants) = self.by_input.get(&entry.hash) {
			let descendants = descendants.iter().filter_map(|hash| by_hash.get(hash));
			for descendant in descendants {
				// if there are no more ancestors of this transaction in the pool
				// => can move from pending to orderings
				if !self.has_in_pool_ancestors(removed, by_hash, &descendant.transaction) {
					self.pending.remove(&descendant.hash);

					if let Some(descendant_entry) = by_hash.get(&descendant.hash) {
						self.ordered.insert_to_orderings(descendant_entry);
					}
				}
			}
		}
		self.by_input.remove(&entry.hash);

		// remove from pending
		self.pending.remove(&entry.hash);

		// remove from orderings
		self.ordered.remove_from_orderings(entry);
	}
}

impl OrderedReferenceStorage {
	pub fn insert_to_orderings(&mut self, entry: &Entry) {
		self.by_storage_index.insert(entry.into());
		self.by_transaction_score.insert(entry.into());
		self.by_package_score.insert(entry.into());
	}

	pub fn remove_from_orderings(&mut self, entry: &Entry) {
		self.by_storage_index.remove(&entry.into());
		self.by_transaction_score.remove(&entry.into());
		self.by_package_score.remove(&entry.into());
	}
}

impl HeapSizeOf for Storage {
	fn heap_size_of_children(&self) -> usize {
		self.by_hash.heap_size_of_children() + self.references.heap_size_of_children()
	}
}

impl HeapSizeOf for ReferenceStorage {
	fn heap_size_of_children(&self) -> usize {
		self.by_input.heap_size_of_children()
			+ self.pending.heap_size_of_children()
			+ self.ordered.heap_size_of_children()
	}
}

impl HeapSizeOf for OrderedReferenceStorage {
	fn heap_size_of_children(&self) -> usize {
		// HeapSizeOf is not implemented for BTreeSet => rough estimation here
		use std::mem::size_of;
		let len = self.by_storage_index.len();
		len * (size_of::<ByTimestampOrderedEntry>()
			+ size_of::<ByTransactionScoreOrderedEntry>()
			+ size_of::<ByPackageScoreOrderedEntry>())
	}
}

impl Default for MemoryPool {
	fn default() -> Self {
		MemoryPool::new()
	}
}

impl MemoryPool {
	/// Creates new memory pool
	pub fn new() -> Self {
		MemoryPool {
			storage: Storage::new(),
			accept_zero_fee_transactions: false,
		}
	}

	/// Accept zero fee transactions.
	pub fn accept_zero_fee_transactions(&mut self) {
		self.accept_zero_fee_transactions = true;
	}

	/// Insert verified transaction to the `MemoryPool`
	pub fn insert_verified<FC: MemoryPoolFeeCalculator>(&mut self, t: IndexedTransaction, fc: &FC) {
		if let Some(entry) = self.make_entry(t, fc) {
			let descendants = self.storage.remove_by_parent_hash(&entry.hash);
			self.storage.insert(entry);
			if let Some(descendants_iter) = descendants.map(|d| d.into_iter()) {
				for descendant in descendants_iter {
					if let Some(descendant_entry) = self.make_entry(descendant, fc) {
						self.storage.insert(descendant_entry);
					}
				}
			}
		}
	}

	/// Iterator over memory pool transactions according to specified strategy
	pub fn iter(&self, strategy: OrderingStrategy) -> MemoryPoolIterator {
		MemoryPoolIterator::new(self, strategy)
	}

	/// Removes single transaction by its hash.
	/// All descendants remain in the pool.
	pub fn remove_by_hash(&mut self, h: &H256) -> Option<IndexedTransaction> {
		self.storage.remove_by_hash(h)
			.map(|entry| IndexedTransaction::new(entry.hash, entry.transaction))
	}

	/// Checks if `transaction` spends some outputs, already spent by inpool transactions.
	pub fn check_double_spend(&self, transaction: &Transaction) -> DoubleSpendCheckResult {
		self.storage.check_double_spend(transaction)
	}

	/// Removes transaction (and all its descendants) which has spent given output
	pub fn remove_by_prevout(&mut self, prevout: &OutPoint) -> Option<Vec<IndexedTransaction>> {
		self.storage.remove_by_prevout(prevout)
	}

	/// Reads single transaction by its hash.
	pub fn read_by_hash(&self, h: &H256) -> Option<&Transaction> {
		self.storage.read_by_hash(h)
	}

	/// Reads hash of the 'top' transaction from the `MemoryPool` using selected strategy.
	/// Ancestors are always returned before descendant transactions.
	pub fn read_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<H256> {
		self.storage.read_with_strategy(strategy)
	}

	/// Reads hashes of up to n transactions from the `MemoryPool`, using selected strategy.
	/// Ancestors are always returned before descendant transactions.
	/// Use this function with care, only if really needed (heavy memory usage)
	pub fn read_n_with_strategy(&mut self, n: usize, strategy: OrderingStrategy) -> Vec<H256> {
		self.iter(strategy).map(|entry| entry.hash.clone()).take(n).collect()
	}

	/// Removes the 'top' transaction from the `MemoryPool` using selected strategy.
	/// Ancestors are always removed before descendant transactions.
	pub fn remove_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<IndexedTransaction> {
		self.storage.remove_with_strategy(strategy)
	}

	/// Removes up to n transactions from the `MemoryPool`, using selected strategy.
	/// Ancestors are always removed before descendant transactions.
	pub fn remove_n_with_strategy(&mut self, n: usize, strategy: OrderingStrategy) -> Vec<IndexedTransaction> {
		self.storage.remove_n_with_strategy(n, strategy)
	}

	/// Set miner virtual fee for transaction
	pub fn set_virtual_fee(&mut self, h: &H256, virtual_fee: i64) {
		self.storage.set_virtual_fee(h, virtual_fee)
	}

	/// Get transaction by hash
	pub fn get(&self, hash: &H256) -> Option<&Transaction> {
		self.storage.get_by_hash(hash).map(|entry| &entry.transaction)
	}

	/// Checks if transaction is in the mempool
	pub fn contains(&self, hash: &H256) -> bool {
		self.storage.contains(hash)
	}

	/// Returns information on `MemoryPool` (as in GetMemPoolInfo RPC)
	/// https://bitcoin.org/en/developer-reference#getmempoolinfo
	pub fn information(&self) -> Information {
		Information {
			transactions_count: self.storage.by_hash.len(),
			transactions_size_in_bytes: self.storage.transactions_size_in_bytes,
		}
	}

	/// Returns TXIDs of all transactions in `MemoryPool` (as in GetRawMemPool RPC)
	/// https://bitcoin.org/en/developer-reference#getrawmempool
	pub fn get_transactions_ids(&self) -> Vec<H256> {
		self.storage.get_transactions_ids()
	}

	/// Returns true if output was spent
	pub fn is_spent(&self, prevout: &OutPoint) -> bool {
		self.storage.is_output_spent(prevout)
	}

	fn make_entry<FC: MemoryPoolFeeCalculator>(&mut self, t: IndexedTransaction, fc: &FC) -> Option<Entry> {
		let ancestors = self.get_ancestors(&t.raw);
		let size = self.get_transaction_size(&t.raw);
		let storage_index = self.get_storage_index();
		let miner_fee = fc.calculate(self, &t.raw);

		// do not accept any transactions that have negative OR zero fee
		if !self.accept_zero_fee_transactions && miner_fee == 0 {
			return None;
		}
		
		Some(Entry {
			transaction: t.raw,
			hash: t.hash,
			ancestors: ancestors,
			storage_index: storage_index,
			size: size,
			miner_fee: miner_fee,
			miner_virtual_fee: 0,
			// following fields are also updated when inserted to storage
			package_size: size,
			package_miner_fee: miner_fee,
			package_miner_virtual_fee: 0,
		})
	}

	fn get_ancestors(&self, t: &Transaction) -> HashSet<H256> {
		let mut ancestors: HashSet<H256> = HashSet::new();
		let ancestors_entries = t.inputs.iter()
			.filter_map(|input| self.storage.get_by_hash(&input.previous_output.hash));
		for ancestor_entry in ancestors_entries {
			ancestors.insert(ancestor_entry.hash.clone());
			for grand_ancestor in &ancestor_entry.ancestors {
				ancestors.insert(grand_ancestor.clone());
			}
		}
		ancestors
	}

	fn get_transaction_size(&self, t: &Transaction) -> usize {
		t.serialized_size()
	}

	#[cfg(not(test))]
	fn get_storage_index(&mut self) -> u64 {
		self.storage.counter += 1;
		self.storage.counter
	}

	#[cfg(test)]
	fn get_storage_index(&self) -> u64 {
		(self.storage.by_hash.len() % 3usize) as u64
	}
}

impl TransactionProvider for MemoryPool {
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.get(hash).map(|t| serialize(t))
	}

	fn transaction(&self, hash: &H256) -> Option<IndexedTransaction> {
		self.get(hash).cloned().map(|tx| IndexedTransaction::new(*hash, tx))
	}
}

impl TransactionOutputProvider for MemoryPool {
	fn transaction_output(&self, prevout: &OutPoint, _transaction_index: usize) -> Option<TransactionOutput> {
		self.get(&prevout.hash)
			.and_then(|tx| tx.outputs.get(prevout.index as usize))
			.cloned()
	}

	fn is_spent(&self, outpoint: &OutPoint) -> bool {
		self.is_spent(outpoint)
	}
}

impl HeapSizeOf for MemoryPool {
	fn heap_size_of_children(&self) -> usize {
		self.storage.heap_size_of_children()
	}
}

pub struct MemoryPoolIterator<'a> {
	memory_pool: &'a MemoryPool,
	references: ReferenceStorage,
	removed: HashSet<H256>,
	strategy: OrderingStrategy,
}

impl<'a> MemoryPoolIterator<'a> {
	fn new(memory_pool: &'a MemoryPool, strategy: OrderingStrategy) -> Self {
		MemoryPoolIterator {
			memory_pool: memory_pool,
			references: memory_pool.storage.references.clone(),
			removed: HashSet::new(),
			strategy: strategy,
		}
	}
}

impl<'a> Iterator for MemoryPoolIterator<'a> {
	type Item = &'a Entry;

	fn next(&mut self) -> Option<Self::Item> {
		let top_hash = match self.strategy {
			OrderingStrategy::ByTimestamp => self.references.ordered.by_storage_index.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByTransactionScore => self.references.ordered.by_transaction_score.iter().map(|entry| entry.hash.clone()).nth(0),
			OrderingStrategy::ByPackageScore => self.references.ordered.by_package_score.iter().map(|entry| entry.hash.clone()).nth(0),
		};

		top_hash.map(|top_hash| {
			let entry = self.memory_pool.storage.by_hash.get(&top_hash).expect("missing hash is a sign of MemoryPool internal inconsistancy");
			self.removed.insert(top_hash.clone());
			self.references.remove(Some(&self.removed), &self.memory_pool.storage.by_hash, entry);
			entry
		})
	}
}

#[cfg(test)]
pub mod tests {
	extern crate test_data;

	use chain::{Transaction, OutPoint};
	use heapsize::HeapSizeOf;
	use fee::NonZeroFeeCalculator;
	use super::{MemoryPool, OrderingStrategy, DoubleSpendCheckResult};
	use self::test_data::{ChainBuilder, TransactionBuilder};

	fn to_memory_pool(chain: &mut ChainBuilder) -> MemoryPool {
		let mut pool = MemoryPool::new();
		for transaction in chain.transactions.iter().cloned() {
			pool.insert_verified(transaction.into(), &NonZeroFeeCalculator);
		}
		pool
	}

	fn default_tx() -> Transaction {
		TransactionBuilder::with_output(1).into()
	}

	#[test]
	fn test_memory_pool_heap_size() {
		let mut pool = MemoryPool::new();

		let size1 = pool.heap_size_of_children();

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		let size2 = pool.heap_size_of_children();
		assert!(size2 > size1);

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		let size3 = pool.heap_size_of_children();
		assert!(size3 > size2);
	}

	#[test]
	fn test_memory_pool_insert_same_transaction() {
		let mut pool = MemoryPool::new();
		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		assert_eq!(pool.get_transactions_ids().len(), 1);

		// insert the same transaction again
		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		assert_eq!(pool.get_transactions_ids().len(), 1);
	}

	#[test]
	fn test_memory_pool_read_with_strategy() {
		let mut pool = MemoryPool::new();
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), Some(default_tx().hash()));
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![default_tx().hash()]);
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), Some(default_tx().hash()));
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![default_tx().hash()]);
	}

	#[test]
	fn test_memory_pool_remove_with_strategy() {
		let mut pool = MemoryPool::new();
		assert_eq!(pool.remove_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		let removed = pool.remove_with_strategy(OrderingStrategy::ByTimestamp);
		assert!(removed.is_some());
		assert_eq!(removed.unwrap(), default_tx().into());

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		let removed = pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp);
		assert_eq!(removed.len(), 1);
		assert_eq!(removed[0], default_tx().into());

		assert_eq!(pool.remove_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);
	}

	#[test]
	fn test_memory_pool_remove_by_hash() {
		let mut pool = MemoryPool::new();

		pool.insert_verified(default_tx().into(), &NonZeroFeeCalculator);
		assert_eq!(pool.get_transactions_ids().len(), 1);

		// remove and check remaining transactions
		let removed = pool.remove_by_hash(&default_tx().hash());
		assert!(removed.is_some());
		assert_eq!(removed.unwrap(), default_tx().into());
		assert_eq!(pool.get_transactions_ids().len(), 0);

		// remove non-existant transaction
		assert_eq!(pool.remove_by_hash(&TransactionBuilder::with_version(1).hash()), None);
		assert_eq!(pool.get_transactions_ids().len(), 0);
	}

	#[test]
	fn test_memory_pool_insert_parent_after_child() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(100).store(chain)
			.into_input(0).add_output(100).store(chain)
			.into_input(0).add_output(100).store(chain);

		// insert child, then parent
		let mut pool = MemoryPool::new();
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator); // timestamp 0
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator); // timestamp 1
		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator); // timestamp 2

		// check that parent transaction was removed before child trnasaction
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0], chain.at(0).into());
		assert_eq!(transactions[1], chain.at(1).into());
		assert_eq!(transactions[2], chain.at(2).into());
	}

	#[test]
	fn test_memory_pool_insert_parent_before_child() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(100).store(chain)
			.into_input(0).add_output(100).store(chain)
			.into_input(0).add_output(100).store(chain);

		// insert parent, then child
		let mut pool = to_memory_pool(chain);

		// check that parent transaction was removed before child trnasaction
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0], chain.at(0).into());
		assert_eq!(transactions[1], chain.at(1).into());
		assert_eq!(transactions[2], chain.at(2).into());
	}

	#[test]
	fn test_memory_pool_insert_child_after_remove_by_hash() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(100).store(chain)
			.into_input(0).add_output(100).store(chain)
			.into_input(0).add_output(100).store(chain);

		// insert parent, then child
		let mut pool = to_memory_pool(chain);

		// remove child transaction & make sure that other transactions are still there
		pool.remove_by_hash(&chain.hash(1));
		assert_eq!(pool.get_transactions_ids().len(), 2);

		// insert child transaction back to the pool & assert transactions are removed in correct order
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTransactionScore);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0], chain.at(0).into());
		assert_eq!(transactions[1], chain.at(1).into());
		assert_eq!(transactions[2], chain.at(2).into());
	}

	#[test]
	fn test_memory_pool_get_information() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(10).store(chain)
			.into_input(0).add_output(20).store(chain)
			.into_input(0).add_output(30).store(chain)
			.into_input(0).add_output(40).store(chain);
		let mut pool = MemoryPool::new();

		let mut transactions_size = 0;
		for transaction_index in 0..4 {
			pool.insert_verified(chain.at(transaction_index).into(), &NonZeroFeeCalculator);
			transactions_size += chain.size(transaction_index);

			let info = pool.information();
			assert_eq!(info.transactions_count, transaction_index + 1);
			assert_eq!(info.transactions_size_in_bytes, transactions_size);
		}
	}

	#[test]
	fn test_memory_pool_timestamp_ordering_strategy() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(10).store(chain)
			.set_output(20).store(chain)
			.set_output(30).store(chain)
			.set_output(40).store(chain);
		let mut pool = to_memory_pool(chain);

		// remove transactions [0, 3, 1] (timestamps: [0, 0, 1]) {conflict resolved by hash}
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0], chain.at(0).into());
		assert_eq!(transactions[1], chain.at(3).into());
		assert_eq!(transactions[2], chain.at(1).into());
		assert_eq!(pool.get_transactions_ids().len(), 1);

		// remove transactions [2] (timestamps: [2])
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 1);
		assert_eq!(transactions[0], chain.at(2).into());
	}

	#[test]
	fn test_memory_pool_transaction_score_ordering_strategy() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(10).store(chain)
			.set_output(40).store(chain)
			.set_output(30).store(chain)
			.set_output(20).store(chain);
		let mut pool = to_memory_pool(chain);

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByTransactionScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0], chain.at(1).into());
		assert_eq!(transactions[1], chain.at(2).into());
		assert_eq!(transactions[2], chain.at(3).into());
		assert_eq!(transactions[3], chain.at(0).into());
	}

	#[test]
	fn test_memory_pool_transaction_score_ordering_strategy_with_virtual_fee() {
		let chain = &mut ChainBuilder::new();
		TransactionBuilder::with_output(10).store(chain)
			.set_output(40).store(chain)
			.set_output(30).store(chain)
			.set_output(20).store(chain);
		let mut pool = to_memory_pool(chain);

		// increase miner score of transaction 3 to move it to position #1
		pool.set_virtual_fee(&chain.hash(3), 100);
		// decrease miner score of transaction 1 to move it to position #4
		pool.set_virtual_fee(&chain.hash(1), -30);

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByTransactionScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0], chain.at(3).into());
		assert_eq!(transactions[1], chain.at(2).into());
		assert_eq!(transactions[2], chain.at(0).into());
		assert_eq!(transactions[3], chain.at(1).into());
	}

	#[test]
	fn test_memory_pool_package_score_ordering_strategy() {
		let chain = &mut ChainBuilder::new();
		// all transactions of same size
		TransactionBuilder::with_default_input(0).set_output(30).store(chain)	// transaction0
			.into_input(0).set_output(50).store(chain)							// transaction0 -> transaction1
			.set_default_input(1).set_output(35).store(chain)					// transaction2
			.into_input(0).set_output(10).store(chain)							// transaction2 -> transaction3
			.into_input(0).set_output(100).store(chain);						// transaction2 -> transaction3 -> transaction4

		let mut pool = MemoryPool::new();

		// compared by simple transaction score:
		// score({ transaction0 }) = 30/60
		// <
		// score({ transaction2 }) = 35/60
		let expected = vec![chain.hash(2), chain.hash(0)];
		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		assert_eq!(pool.read_n_with_strategy(2, OrderingStrategy::ByPackageScore), expected);

		// { transaction0, transaction1 } now have bigger score than { transaction2 }:
		// score({ transaction0, transaction1 }) = (30 + 50) / 120 ~ 0.667
		// >
		// score({ transaction2 }) = 35/60 ~ 0.583
		// => chain1 is boosted
		// => so transaction with lesser individual score (but with bigger package score) is mined first
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(0), chain.hash(1), chain.hash(2)];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// { transaction0, transaction1 } still have bigger score than { transaction2, transaction3 }
		// score({ transaction0, transaction1 }) = (30 + 35) / 120 ~ 0.625
		// >
		// score({ transaction2, transaction3 }) = (35 + 10) / 120 ~ 0.375
		// => chain2 is not boosted
		pool.insert_verified(chain.at(3).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(0), chain.hash(1), chain.hash(2), chain.hash(3)];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);

		// { transaction0, transaction1 } now have lesser score than { transaction2, transaction3, transaction4 }
		// score({ transaction0, transaction1 }) = (30 + 35) / 120 ~ 0.625
		// <
		// score({ transaction2, transaction3, transaction4 }) = (35 + 10 + 100) / 180 ~ 0.806
		// => chain2 is boosted
		pool.insert_verified(chain.at(4).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(2), chain.hash(3), chain.hash(4), chain.hash(0), chain.hash(1)];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);

		// add virtual fee to the transaction1 so that chain1 is back to the position #1
		pool.set_virtual_fee(&chain.hash(1), 500i64);
		let expected = vec![chain.hash(0), chain.hash(1), chain.hash(2), chain.hash(3), chain.hash(4)];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);
	}

	#[test]
	fn test_memory_pool_package_score_ordering_strategy_opposite_insert_order() {
		let chain = &mut ChainBuilder::new();
		// all transactions of same size
		TransactionBuilder::with_default_input(0).set_output(17).store(chain)	// transaction0
			.into_input(0).set_output(50).store(chain)							// transaction0 -> transaction1
			.into_input(0).set_output(7).store(chain)							// transaction0 -> transaction1 -> transaction2
			.set_default_input(1).set_output(20).store(chain);					// transaction3

		let mut pool = MemoryPool::new();

		// transaction0 is not linked to the transaction2
		// => they are in separate chains now
		// => transaction3 has greater score than both of these chains
		pool.insert_verified(chain.at(3).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(3), chain.hash(0), chain.hash(2)];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// insert the missing transaction to link together chain1
		// => it now will have better score than chain2
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(0), chain.hash(1), chain.hash(3), chain.hash(2)];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);
	}

	#[test]
	fn test_memory_pool_complex_transactions_tree_opposite_insert_order() {
		let chain = &mut ChainBuilder::new();
		// all transactions of same size (=> 3 inputs)
		// construct level0
		TransactionBuilder::with_default_input(0).add_default_input(1).add_default_input(2).set_output(10).add_output(10).store(chain)		// transaction0
			.set_default_input(3).add_default_input(4).add_default_input(5).set_output(20).add_output(20).store(chain)						// transaction1
			.set_default_input(6).add_default_input(7).add_default_input(8).set_output(30).add_output(30).store(chain)						// transaction2
			// construct level1
			.set_default_input(9).add_default_input(10).add_input(&chain.at(0), 0).set_output(40).add_output(40).store(chain)				// transaction0 -> transaction3
			.set_default_input(11).add_input(&chain.at(0), 1).add_input(&chain.at(1), 0).set_output(50).add_output(50).store(chain)			// transaction0 + transaction1 -> transaction4
			// construct level3
			.set_input(&chain.at(2), 0).add_input(&chain.at(3), 0).add_input(&chain.at(4), 0).set_output(60).add_output(60).store(chain);	// transaction2 + transaction3 + transaction4 -> transaction5

		let mut pool = MemoryPool::new();

		// insert level1 + level2. There are two chains:
		// score({ transaction3, transaction5 }) = 40 + 60
		// score({ transaction4, transaction5 }) = 50 + 60
		pool.insert_verified(chain.at(5).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(3).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(4).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(4), chain.hash(3), chain.hash(5)];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Three chains:
		// score({ transaction3, transaction5 }) = 40 + 60
		// score({ transaction4, transaction5 }) = 50 + 60
		// score({ transaction2, transaction5 }) = 30 + 60
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(4), chain.hash(3), chain.hash(2), chain.hash(5)];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Three chains:
		// score({ transaction3, transaction5 }) = 40 + 60 / 2 = 0.5
		// score({ transaction1, transaction4, transaction5 }) = 20 + 50 + 60 / 3 ~ 0.333
		// score({ transaction2, transaction5 }) = 30 + 60 / 2 = 0.45
		// but second chain will be removed first anyway because previous #1 ({ transaction4, transaction5}) now depends on level 01
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(3), chain.hash(2), chain.hash(1), chain.hash(4), chain.hash(5)];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Four chains:
		// score({ transaction0, transaction3, transaction5 }) = (10 + 40 + 60) / (60 + 60 + 142) ~ 0.420
		// score({ transaction0, transaction4, transaction5 }) = (10 + 50 + 60) / (60 + 60 + 142) ~ 0.458
		// score({ transaction1, transaction3, transaction5 }) = (20 + 50 + 60) / (60 + 60 + 142) ~ 0.496
		// score({ transaction2, transaction5 }) = (30 + 60) / (60 + 142) ~ 0.445
		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator);
		let expected = vec![chain.hash(2), chain.hash(1), chain.hash(0), chain.hash(4), chain.hash(3), chain.hash(5)];
		assert_eq!(pool.read_n_with_strategy(6, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(6, OrderingStrategy::ByPackageScore), expected);
	}

	#[test]
	fn test_memory_pool_spent_transaction_output() {
		let chain = &mut ChainBuilder::new();

		// all transactions of same size (=> 3 inputs)
		// construct level0
		TransactionBuilder::with_output(10).store(chain)				// transaction0
			.set_output(20).store(chain)								// transaction1
			.set_input(&chain.at(0), 0).add_output(30).store(chain);	// transaction0 -> transaction2

		let mut pool = MemoryPool::new();
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(0), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(1), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(2), index: 0, }));

		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator);
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(0), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(1), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(2), index: 0, }));

		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(0), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(1), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(2), index: 0, }));

		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		assert!(pool.is_spent(&OutPoint { hash: chain.hash(0), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(1), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(2), index: 0, }));

		pool.remove_by_hash(&chain.at(2).hash());
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(0), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(1), index: 0, }));
		assert!(!pool.is_spent(&OutPoint { hash: chain.hash(2), index: 0, }));
	}

	#[test]
	fn test_memory_pool_remove_by_prevout() {
		let chain = &mut ChainBuilder::new();

		// all transactions of same size (=> 3 inputs)
		// construct level0
		TransactionBuilder::with_output(10).store(chain)	// transaction0
			.into_input(0).add_output(20).store(chain)		// transaction0 -> transaction1
			.into_input(0).add_output(30).store(chain)		// transaction0 -> transaction1 -> transaction2
			.reset().add_output(40).store(chain);			// transaction3
		let mut pool = MemoryPool::new();

		pool.insert_verified(chain.at(0).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(3).into(), &NonZeroFeeCalculator);
		assert_eq!(pool.information().transactions_count, 4);

		assert_eq!(pool.remove_by_prevout(&OutPoint { hash: chain.hash(0), index: 0 }), Some(vec![chain.at(1).into(), chain.at(2).into()]));
		assert_eq!(pool.information().transactions_count, 2);
	}

	#[test]
	fn test_memory_pool_check_double_spend() {
		let chain = &mut ChainBuilder::new();

		TransactionBuilder::with_output(10).add_output(10).add_output(10).store(chain)	// t0
			.reset().set_input(&chain.at(0), 0).add_output(20).lock().store(chain)		// nonfinal: t0[0] -> t1
			.reset().set_input(&chain.at(1), 0).add_output(30).store(chain)				// dependent: t0[0] -> t1[0] -> t2
			.reset().set_input(&chain.at(0), 0).add_output(40).store(chain)				// good replacement: t0[0] -> t3
			.reset().set_input(&chain.at(0), 1).add_output(50).store(chain)				// final: t0[1] -> t4
			.reset().set_input(&chain.at(0), 1).add_output(60).store(chain)				// bad replacement: t0[1] -> t5
			.reset().set_input(&chain.at(0), 2).add_output(70).store(chain);			// no double spend: t0[2] -> t6

		let mut pool = MemoryPool::new();
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(2).into(), &NonZeroFeeCalculator);
		pool.insert_verified(chain.at(4).into(), &NonZeroFeeCalculator);
		// when output is spent by nonfinal transaction
		match pool.check_double_spend(&chain.at(3)) {
			DoubleSpendCheckResult::NonFinalDoubleSpend(set) => {
				assert_eq!(set.double_spends.len(), 1);
				assert!(set.double_spends.contains(&chain.at(1).inputs[0].previous_output.clone().into()));
				assert_eq!(set.dependent_spends.len(), 2);
				assert!(set.dependent_spends.contains(&OutPoint {
					hash: chain.at(1).hash(),
					index: 0,
				}.into()));
				assert!(set.dependent_spends.contains(&OutPoint {
					hash: chain.at(2).hash(),
					index: 0,
				}.into()));
			},
			_ => panic!("unexpected"),
		}
		// when output is spent by final transaction
		match pool.check_double_spend(&chain.at(5)) {
			DoubleSpendCheckResult::DoubleSpend(inpool_hash, prev_hash, prev_index) => {
				assert_eq!(inpool_hash, chain.at(4).hash());
				assert_eq!(prev_hash, chain.at(0).hash());
				assert_eq!(prev_index, 1);
			},
			_ => panic!("unexpected"),
		}
		// when output is not spent at all
		match pool.check_double_spend(&chain.at(6)) {
			DoubleSpendCheckResult::NoDoubleSpend => (),
			_ => panic!("unexpected"),
		}
	}

	#[test]
	fn test_memory_pool_check_double_spend_multiple_dependent_outputs() {
		let chain = &mut ChainBuilder::new();

		TransactionBuilder::with_output(100).store(chain)															// t0
			.reset().set_input(&chain.at(0), 0).add_output(20).add_output(30).add_output(50).lock().store(chain)	// nonfinal: t0[0] -> t1
			.reset().set_input(&chain.at(0), 0).add_output(40).store(chain);										// good replacement: t0[0] -> t2

		let mut pool = MemoryPool::new();
		pool.insert_verified(chain.at(1).into(), &NonZeroFeeCalculator);

		// when output is spent by nonfinal transaction
		match pool.check_double_spend(&chain.at(2)) {
			DoubleSpendCheckResult::NonFinalDoubleSpend(set) => {
				assert_eq!(set.double_spends.len(), 1);
				assert!(set.double_spends.contains(&chain.at(1).inputs[0].previous_output.clone().into()));
				assert_eq!(set.dependent_spends.len(), 3);
				assert!(set.dependent_spends.contains(&OutPoint {
					hash: chain.at(1).hash(),
					index: 0,
				}.into()));
				assert!(set.dependent_spends.contains(&OutPoint {
					hash: chain.at(1).hash(),
					index: 1,
				}.into()));
				assert!(set.dependent_spends.contains(&OutPoint {
					hash: chain.at(1).hash(),
					index: 2,
				}.into()));
			},
			_ => panic!("unexpected"),
		}

	}

	#[test]
	fn test_memory_pool_is_spent() {
		let tx1: Transaction = TransactionBuilder::with_default_input(0).set_output(1).into();
		let tx2: Transaction = TransactionBuilder::with_default_input(1).set_output(1).into();
		let out1 = tx1.inputs[0].previous_output.clone();
		let out2 = tx2.inputs[0].previous_output.clone();
		let mut memory_pool = MemoryPool::new();
		memory_pool.insert_verified(tx1.into(), &NonZeroFeeCalculator);
		assert!(memory_pool.is_spent(&out1));
		assert!(!memory_pool.is_spent(&out2));
	}
}
