//! Transactions memory pool
//!
//! `MemoryPool` keeps track of all transactions seen by the node (received from other peers) and own transactions
//! and orders them by given strategies. It works like multi-indexed priority queue, giving option to pop 'top'
//! transactions.
//! It also guarantees that ancestor-descendant relation won't break during ordered removal (ancestors always removed
//! before descendants). Removal using remove_by_hash can break this rule.
use hash::H256;
use chain::Transaction;
use std::cmp::Ordering;
use std::collections::HashMap;
use std::collections::HashSet;
use ser::Serializable;

/// Transactions ordering strategy
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
	/// The number of transactions currently in the `MemoryPool`
	pub transactions_count: usize,
	/// The total number of bytes in the transactions in the `MemoryPool`
	pub transactions_size_in_bytes: usize,
}

/// Transactions memory pool
#[derive(Debug)]
pub struct MemoryPool {
	/// Transactions storage
	storage: Storage,
}

/// Single entry
#[derive(Debug)]
pub struct Entry {
	/// Transaction
	transaction: Transaction,
	/// In-pool ancestors hashes for this transaction
	ancestors: HashSet<H256>,
	/// Transaction hash (stored for effeciency)
	hash: H256,
	/// Transaction size (stored for effeciency)
	size: usize,
	/// Throughout index of this transaction in memory pool (non persistent)
	storage_index: u64,
	/// Transaction fee (stored for efficiency)
	miner_fee: i64,
	/// Virtual transaction fee (a way to prioritize/penalize transaction)
	miner_virtual_fee: i64,
	/// size + Sum(size) for all in-pool descendants
	package_size: usize,
	/// miner_fee + Sum(miner_fee) for all in-pool descendants
	package_miner_fee: i64,
	/// miner_virtual_fee + Sum(miner_virtual_fee) for all in-pool descendants
	package_miner_virtual_fee: i64,
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
	/// References storage
	references: ReferenceStorage,
}

#[derive(Debug, Clone)]
struct ReferenceStorage {
	/// By-input storage
	by_input: HashMap<H256, HashSet<H256>>,
	/// Pending entries
	pending: HashSet<H256>,
	/// By-entry-time storage
	by_storage_index: storage_index_strategy::Storage,
	/// By-score storage
	by_transaction_score: transaction_score_strategy::Storage,
	/// By-package-score strategy
	by_package_score: package_score_strategy::Storage,
}

macro_rules! ordering_strategy {
	($strategy: ident; $($member: ident: $member_type: ty), *; $comparer: expr) => {
		mod $strategy {
			use std::cmp::Ordering;
			use hash::H256;
			use std::collections::BTreeSet;
			use super::Entry;

			/// Lightweight struct maintain transactions ordering
			#[derive(Debug, Eq, PartialEq, Clone)]
			pub struct OrderedEntry {
				/// Transaction hash
				hash: H256,
				/// Transaction data
				$($member: $member_type), *
			}

			impl OrderedEntry {
				pub fn for_entry(entry: &Entry) -> OrderedEntry {
					OrderedEntry {
						hash: entry.hash.clone(),
						$($member: entry.$member.clone()), *
					}
				}
			}

			impl PartialOrd for OrderedEntry {
				fn partial_cmp(&self, other: &OrderedEntry) -> Option<Ordering> {
					Some(self.cmp(other))
				}
			}

			impl Ord for OrderedEntry {
				fn cmp(&self, other: &Self) -> Ordering {
					let order = $comparer(&self, other);
					if order != Ordering::Equal {
						return order
					}

					self.hash.cmp(&other.hash)
				}
			}

			/// Ordering storage
			#[derive(Debug, Clone)]
			pub struct Storage {
				data: BTreeSet<OrderedEntry>,
			}

			impl Storage {
				pub fn new() -> Self {
					Storage {
						data: BTreeSet::new()
					}
				}

				/// Insert entry to storage
				pub fn insert(&mut self, entry: &Entry) {
					self.data.replace(OrderedEntry::for_entry(entry));
				}

				/// Remove entry from storage
				pub fn remove(&mut self, entry: &Entry) -> bool {
					self.data.remove(&OrderedEntry::for_entry(entry))
				}

				/// Returns hash of the top entry
				pub fn top(&self) -> Option<H256> {
					self.data.iter().map(|ref entry| entry.hash.clone()).nth(0)
				}
			}
		}
	}
}

// Ordering strategies declaration

ordering_strategy!(storage_index_strategy;
	storage_index: u64;
	|me: &Self, other: &Self|
		me.storage_index.cmp(&other.storage_index));
ordering_strategy!(transaction_score_strategy;
	size: usize, miner_fee: i64, miner_virtual_fee: i64;
	|me: &Self, other: &Self| {
		// lesser miner score means later removal
		let left = (me.miner_fee + me.miner_virtual_fee) * (other.size as i64);
		let right = (other.miner_fee + other.miner_virtual_fee) * (me.size as i64);
		right.cmp(&left)
	});
ordering_strategy!(package_score_strategy;
	package_size: usize, package_miner_fee: i64, package_miner_virtual_fee: i64;
	|me: &Self, other: &Self| {
		// lesser miner score means later removal
		let left = (me.package_miner_fee + me.package_miner_virtual_fee) * (other.package_size as i64);
		let right = (other.package_miner_fee + other.package_miner_virtual_fee) * (me.package_size as i64);
		right.cmp(&left)
	});

// Macro to use instead of member functions (to deal with double references)

macro_rules! insert_to_orderings {
	($me: expr, $entry: expr) => (
		$me.by_storage_index.insert(&$entry);
		$me.by_transaction_score.insert(&$entry);
		$me.by_package_score.insert(&$entry);
	)
}

macro_rules! remove_from_orderings {
	($me: expr, $entry: expr) => (
		$me.by_storage_index.remove(&$entry);
		$me.by_transaction_score.remove(&$entry);
		$me.by_package_score.remove(&$entry);
	)
}

impl Storage {
	pub fn new() -> Self {
		Storage {
			counter: 0,
			transactions_size_in_bytes: 0,
			by_hash: HashMap::new(),
			references: ReferenceStorage {
				by_input: HashMap::new(),
				pending: HashSet::new(),
				by_storage_index: storage_index_strategy::Storage::new(),
				by_transaction_score: transaction_score_strategy::Storage::new(),
				by_package_score: package_score_strategy::Storage::new(),
			},
		}
	}

	pub fn insert(&mut self, entry: Entry) {
		// update pool information
		self.transactions_size_in_bytes += entry.size;

		// remember that this transactions depends on its inputs
		for input_hash in entry.transaction.inputs.iter().map(|input| &input.previous_output.hash) {
			self.references.by_input.entry(input_hash.clone()).or_insert_with(|| HashSet::new()).insert(entry.hash.clone());
		}

		// update score of all packages this transaction is in
		for ancestor_hash in entry.ancestors.iter() {
			if let Some(ref mut ancestor_entry) = self.by_hash.get_mut(ancestor_hash) {
				let removed = self.references.by_package_score.remove(ancestor_entry);

				ancestor_entry.package_size += entry.size;
				ancestor_entry.package_miner_fee += entry.package_miner_fee;
				ancestor_entry.package_miner_virtual_fee += entry.package_miner_virtual_fee;

				if removed {
					self.references.by_package_score.insert(ancestor_entry);
				}
			}
		}

		// insert either to pending queue or to orderings
		if Storage::has_in_pool_ancestors(None, &self.by_hash, &entry.transaction) {
			self.references.pending.insert(entry.hash.clone());
		}
		else {
			insert_to_orderings!(self.references, entry);
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

	pub fn set_virtual_fee(&mut self, h: &H256, virtual_fee: i64) {
		// for updating ancestors
		let mut miner_virtual_fee_change = 0i64;
		let mut ancestors: Option<Vec<H256>> = None;

		// modify the entry itself
		if let Some(ref mut entry) = self.by_hash.get_mut(h) {
			let insert_to_package_score = self.references.by_package_score.remove(&entry);
			let insert_to_transaction_score = self.references.by_transaction_score.remove(&entry);

			miner_virtual_fee_change = virtual_fee - entry.miner_virtual_fee;
			if !entry.ancestors.is_empty() {
				ancestors = Some(entry.ancestors.iter().cloned().collect());
			}

			entry.miner_virtual_fee = virtual_fee;

			if insert_to_transaction_score {
				self.references.by_transaction_score.insert(&entry);
			}
			if insert_to_package_score {
				self.references.by_package_score.insert(&entry);
			}
		}

		// now modify all ancestor entries
		if miner_virtual_fee_change != 0i64 {
			ancestors.map(|ancestors| {
				for ancestor_hash in ancestors {
					if let Some(ref mut ancestor_entry) = self.by_hash.get_mut(&ancestor_hash) {
						let insert_to_package_score = self.references.by_package_score.remove(ancestor_entry);
						ancestor_entry.package_miner_virtual_fee += miner_virtual_fee_change;
						if insert_to_package_score {
							self.references.by_package_score.insert(ancestor_entry);
						}
					}
				}
			});
		}
	}

	pub fn read_with_strategy(&self, strategy: OrderingStrategy) -> Option<H256> {
		match strategy {
			OrderingStrategy::ByTimestamp => self.references.by_storage_index.top(),
			OrderingStrategy::ByTransactionScore => self.references.by_transaction_score.top(),
			OrderingStrategy::ByPackageScore => self.references.by_package_score.top(),
		}
	}

	pub fn read_n_with_strategy(&self, mut n: usize, strategy: OrderingStrategy) -> Vec<H256> {
		if n == 0 {
			return Vec::new();
		}

		if n == 1 {
			return self.read_with_strategy(strategy)
				.map_or(Vec::new(), |h| vec![h]);
		}

		let mut references = self.references.clone();
		let mut result: Vec<H256> = Vec::new();
		let mut removed: HashSet<H256> = HashSet::new();
		loop {
			if n == 0 {
				break;
			}
			n -= 1;

			let top_hash = match strategy {
				OrderingStrategy::ByTimestamp => references.by_storage_index.top(),
				OrderingStrategy::ByTransactionScore => references.by_transaction_score.top(),
				OrderingStrategy::ByPackageScore => references.by_package_score.top(),
			};
			match top_hash {
				None => break,
				Some(top_hash) => {
					self.by_hash.get(&top_hash).map(|entry| {
						// simulate removal
						removed.insert(top_hash.clone());
						Storage::remove(Some(&removed), &self.by_hash, &mut references, &entry);

						// return this entry
						result.push(top_hash);
					});
				},
			}
		}
		result
	}

	pub fn remove_by_hash(&mut self, h: &H256) -> Option<Entry> {
		self.by_hash.remove(h)
			.map(|entry| {
				// update pool information
				self.transactions_size_in_bytes -= entry.size;

				// remove from storage
				Storage::remove(None, &self.by_hash, &mut self.references, &entry);

				entry
			})
	}

	pub fn remove_by_parent_hash(&mut self, h: &H256) -> Option<Vec<Transaction>> {
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
					.filter_map(|hash| self.remove_by_hash(&hash).map(|entry| entry.transaction))
					.collect())
		}
		else {
			None
		}
	}

	pub fn remove_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<Transaction> {
		let top_hash = match strategy {
			OrderingStrategy::ByTimestamp => self.references.by_storage_index.top(),
			OrderingStrategy::ByTransactionScore => self.references.by_transaction_score.top(),
			OrderingStrategy::ByPackageScore => self.references.by_package_score.top(),
		};
		top_hash.map(|hash| self.remove_by_hash(&hash)
			.expect("`hash` is read from `references`; entries in `references` have corresponging entries in `by_hash`; `remove_by_hash` removes entry from `by_hash`; qed")
			.transaction)
	}

	pub fn remove_n_with_strategy(&mut self, mut n: usize, strategy: OrderingStrategy) -> Vec<Transaction> {
		let mut result: Vec<Transaction> = Vec::new();
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
		self.by_hash.keys().map(|h| h.clone()).collect()
	}

	fn remove(removed: Option<&HashSet<H256>>, by_hash: &HashMap<H256, Entry>, references: &mut ReferenceStorage, entry: &Entry) {
		// for each pending descendant transaction
		if let Some(descendants) = references.by_input.get(&entry.hash) {
			let descendants = descendants.iter().filter_map(|hash| by_hash.get(&hash));
			for descendant in descendants {
				// if there are no more ancestors of this transaction in the pool
				// => can move from pending to orderings
				if !Storage::has_in_pool_ancestors(removed, by_hash, &descendant.transaction) {
					references.pending.remove(&descendant.hash);

					if let Some(descendant_entry) = by_hash.get(&descendant.hash) {
						insert_to_orderings!(references, &descendant_entry);
					}
				}
			}
		}
		references.by_input.remove(&entry.hash);

		// remove from pending
		references.pending.remove(&entry.hash);

		// remove from orderings
		remove_from_orderings!(references, entry);
	}

	fn has_in_pool_ancestors(removed: Option<&HashSet<H256>>, by_hash: &HashMap<H256, Entry>, transaction: &Transaction) -> bool {
		transaction.inputs.iter()
			.any(|input| by_hash.contains_key(&input.previous_output.hash)
				&& !removed.map_or(false, |r| r.contains(&input.previous_output.hash)))
	}
}

impl MemoryPool {
	/// Creates new memory pool
	pub fn new() -> Self {
		MemoryPool {
			storage: Storage::new(),
		}
	}

	/// Insert verified transaction to the `MemoryPool`
	pub fn insert_verified(&mut self, t: Transaction) {
		let entry = self.make_entry(t);
		let descendants = self.storage.remove_by_parent_hash(&entry.hash);
		self.storage.insert(entry);
		if let Some(descendants_iter) = descendants.map(|d| d.into_iter()) {
			for descendant in descendants_iter {
				let descendant_entry = self.make_entry(descendant);
				self.storage.insert(descendant_entry);
			}
		}
	}

	/// Removes single transaction by its hash.
	/// All descedants remain in the pool.
	pub fn remove_by_hash(&mut self, h: &H256) -> Option<Transaction> {
		self.storage.remove_by_hash(h).map(|entry| entry.transaction)
	}

<<<<<<< HEAD
	/// Reads hash of the 'top' transaction from the `MemoryPool` using selected strategy.
	/// Ancestors are always returned before descendant transactions.
	pub fn read_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<H256> {
		self.storage.read_with_strategy(strategy)
	}

	/// Reads hashes of up to n transactions from the `MemoryPool`, using selected strategy.
	/// Ancestors are always returned before descendant transactions.
	pub fn read_n_with_strategy(&mut self, n: usize, strategy: OrderingStrategy) -> Vec<H256> {
		self.storage.read_n_with_strategy(n, strategy)
	}

	/// Removes the 'top' transaction from the `MemoryPool` using selected strategy.
	/// Ancestors are always removed before descendant transactions.
	pub fn remove_with_strategy(&mut self, strategy: OrderingStrategy) -> Option<Transaction> {
		self.storage.remove_with_strategy(strategy)
	}

	/// Removes up to n transactions from the `MemoryPool`, using selected strategy.
=======
	/// Removes up to n transactions the `MemoryPool`, using selected strategy.
>>>>>>> origin/master
	/// Ancestors are always removed before descendant transactions.
	pub fn remove_n_with_strategy(&mut self, n: usize, strategy: OrderingStrategy) -> Vec<Transaction> {
		self.storage.remove_n_with_strategy(n, strategy)
	}

	/// Set miner virtual fee for transaction
	pub fn set_virtual_fee(&mut self, h: &H256, virtual_fee: i64) {
		self.storage.set_virtual_fee(h, virtual_fee)
	}

	/// Get transaction by hash
	pub fn get(&self, hash: &H256) -> Option<&Transaction> {
		self.storage.get_by_hash(hash).map(|ref entry| &entry.transaction)
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
			transactions_size_in_bytes: self.storage.transactions_size_in_bytes
		}
	}

	/// Returns TXIDs of all transactions in `MemoryPool` (as in GetRawMemPool RPC)
	/// https://bitcoin.org/en/developer-reference#getrawmempool
	pub fn get_transactions_ids(&self) -> Vec<H256> {
		self.storage.get_transactions_ids()
	}

	fn make_entry(&mut self, t: Transaction) -> Entry {
		let hash = t.hash();
		let ancestors = self.get_ancestors(&t);
		let size = self.get_transaction_size(&t);
		let storage_index = self.get_storage_index();
		let miner_fee = self.get_transaction_miner_fee(&t);
		Entry {
			transaction: t,
			hash: hash,
			ancestors: ancestors,
			storage_index: storage_index,
			size: size,
			miner_fee: miner_fee,
			miner_virtual_fee: 0,
			// following fields are also updated when inserted to storage
			package_size: size,
			package_miner_fee: miner_fee,
			package_miner_virtual_fee: 0,
		}
	}

	fn get_ancestors(&self, t: &Transaction) -> HashSet<H256> {
		let mut ancestors: HashSet<H256> = HashSet::new();
		let ancestors_entries = t.inputs.iter()
			.filter_map(|ref input| self.storage.get_by_hash(&input.previous_output.hash));
		for ancestor_entry in ancestors_entries {
			ancestors.insert(ancestor_entry.hash.clone());
			for grand_ancestor in ancestor_entry.ancestors.iter() {
				ancestors.insert(grand_ancestor.clone());
			}
		}
		ancestors
	}

	fn get_transaction_size(&self, t: &Transaction) -> usize {
		t.serialized_size()
	}

	fn get_transaction_miner_fee(&self, t: &Transaction) -> i64 {
		let input_value = 0; // TODO: sum all inputs of transaction
		let output_value = t.outputs.iter().fold(0, |acc, ref output| acc + output.value);
		(output_value - input_value) as i64
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

#[cfg(test)]
mod tests {
	use std::cmp::Ordering;
	use hash::H256;
	use chain::Transaction;
	use super::{MemoryPool, OrderingStrategy};

	// output_value = 898126612, size = 225, miner_score ~ 3991673.83
	const RAW_TRANSACTION1: &'static str = "01000000017e4e1bfa4cc16a46593390b9f627db9a3b7c3b1802daa826fc0c477c067ea4f1000000006a47304402203cca1b01b307d3dba3d4f819ef4e9ccf839fa7ef901fc39d2b1d6e33c159a0b0022009d135bd47b8465a69f4db7145af34e0f063e926d95d9c21fb4e8cbc2052838c0121039c01d413f0e296cb766b408c528d3526e75a0f63cfc44c1147160613a12e6cb7feffffff02c8f27535000000001976a914e54788b730b91eb9b29917fa10ddbc97996a987988ac4c601200000000001976a91421e8ed0ddc9a365c10fa3130c91c36237a45848888aca7a00600";
	// output_value = 423675406, size = 225, miner_score ~ 1883001.80
	const RAW_TRANSACTION2: &'static str = "0100000001efaf295b354d8063336a03652664e31b63666f6fbe51b377ad3bdd7b65678a43000000006a47304402206100084828cfc2b71881f1a99447658d5844043f69c1f9bd6c95cb0e11197d4002207c8e23b6233e7317fe0db3e12c8a4291efe671e02c4bbeeb3534e208bb9a3a75012103e091c9c970427a709646990ca16d4d418efc9e5c46ae794c3d09023a7e0e1c57feffffff0226662e19000000001976a914ffad82403bc2a1dd3789b7e653d352515ae86b7288ace85f1200000000001976a914c2f8a6513ebcf6f61edca10199442e33108d540988aca7a00600";
	// output_value = 668388826, size = 225, miner_score ~ 2970617.00
	const RAW_TRANSACTION3: &'static str = "01000000012eee0922c7385e6a11d4f92da65e16b205b2cdfe294c3140c3bc0427f6c11794000000006a47304402207aab34b1c9bb5464c3c1ddf9fee042d8755b81ed1e8c35b895ee2d7da17a23ac02205dfd9c8d14f44951c9e8c3a70f8820a6b113046db1ca25d52a31a4b68d62e07901210306a47c3c0ce5ad78616ad0695113ee4d7a848155fd9f4a1eb7aeed756e174211feffffff024c601200000000001976a914ae829d4d1a8945dc165a57f1149b4656e48c161988ac8e6dc427000000001976a9146c4f1f52adec5211f456acc12917fe902949b08088aca7a00600";
	// output_value = 256505044, size = 226, miner_score ~ 1134978.07
	const RAW_TRANSACTION4: &'static str = "01000000012969032b2a920c03fa71eeea5250d0f6259b130a15ed695f79d957e47b9b2d8b000000006b483045022100b34c8a714b295b84686211b37daaa65ef79cf0ce1dc7d7a4e133a5b08a308f6f02201abca9e08bddb56e205bd1c5e689419926031ec955d8c89671a16a7076dce0ec0121020db95d68c760d1e0a5090dbf0ed2cbfd1225c3bc46d4e31e272bcc14b42a9643feffffff021896370f000000001976a914552b2549642c22462dc0e82ea25500dea6bb5e2188acbc5e1200000000001976a914dca40d275f5eb00cefab813925a1b07b9b77159188aca7a00600";
	const RAW_TRANSACTION1_HASH: &'static str = "af9d5d566b6c914fc0de758a5f18731f5570ac59d1f0c5d1516f0f2dda2c3f79";
	const RAW_TRANSACTION2_HASH: &'static str = "e97719095d91821f691dcbebdf5fb82c4eff8dd33d0c3cc6690aae37ed82a01e";
	const RAW_TRANSACTION3_HASH: &'static str = "2d3833b35efc8f2b9d1c2140505b207fd71155178ad604e03364448f7007fc04";
	const RAW_TRANSACTION4_HASH: &'static str = "4a15e0b41b1a47381f2f2baa16087688d1cd9078416960deed1faae852a469ce";

	fn construct_memory_pool() -> MemoryPool {
		let transaction1: Transaction = RAW_TRANSACTION1.into();
		let transaction2: Transaction = RAW_TRANSACTION2.into();
		let transaction3: Transaction = RAW_TRANSACTION3.into();
		let transaction4: Transaction = RAW_TRANSACTION4.into();
		let transaction1_hash =  RAW_TRANSACTION1_HASH.into();
		let transaction4_hash =  RAW_TRANSACTION4_HASH.into();
		assert_eq!(transaction1.hash(), transaction1_hash);
		assert_eq!(transaction2.hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(transaction3.hash(), RAW_TRANSACTION3_HASH.into());
		assert_eq!(transaction4.hash(), transaction4_hash);

		// hash of t4 must be lesser than hash of t4
		assert_eq!(transaction4_hash.cmp(&transaction1_hash), Ordering::Less);

		let mut pool = MemoryPool::new();
		pool.insert_verified(RAW_TRANSACTION1.into());
		pool.insert_verified(RAW_TRANSACTION2.into());
		pool.insert_verified(RAW_TRANSACTION3.into());
		pool.insert_verified(RAW_TRANSACTION4.into());

		pool
	}

	#[test]
	fn test_memory_pool_insert_same_transaction() {
		let mut pool = MemoryPool::new();
		pool.insert_verified(RAW_TRANSACTION1.into());
		assert_eq!(pool.get_transactions_ids().len(), 1);

		// insert the same transaction again
		pool.insert_verified(RAW_TRANSACTION1.into());
		assert_eq!(pool.get_transactions_ids().len(), 1);
	}

	#[test]
	fn test_memory_pool_read_with_strategy() {
		let mut pool = MemoryPool::new();
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);

		let t: Transaction = "00000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000011e000000000000000000000000".into();
		let thash = t.hash();
		pool.insert_verified(t);
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), Some(thash.clone()));
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![thash.clone()]);
		assert_eq!(pool.read_with_strategy(OrderingStrategy::ByTimestamp), Some(thash.clone()));
		assert_eq!(pool.read_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![thash.clone()]);
	}

	#[test]
	fn test_memory_pool_remove_with_strategy() {
		let mut pool = MemoryPool::new();
		assert_eq!(pool.remove_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);

		let raw_transaction = "00000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000011e000000000000000000000000";
		let transaction: Transaction = raw_transaction.into();
		let hash = transaction.hash();

		pool.insert_verified(transaction);
		let removed = pool.remove_with_strategy(OrderingStrategy::ByTimestamp);
		assert!(removed.is_some());
		assert_eq!(removed.unwrap().hash(), hash);

		pool.insert_verified(raw_transaction.into());
		let removed = pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp);
		assert_eq!(removed.len(), 1);
		assert_eq!(removed[0].hash(), hash);

		assert_eq!(pool.remove_with_strategy(OrderingStrategy::ByTimestamp), None);
		assert_eq!(pool.remove_n_with_strategy(100, OrderingStrategy::ByTimestamp), vec![]);
	}

	#[test]
	fn test_memory_pool_remove_by_hash() {
		let mut pool = construct_memory_pool();

		let pool_transactions = pool.get_transactions_ids();
		assert_eq!(pool_transactions.len(), 4);

		// check pool transactions
		let ref hash_to_remove = pool_transactions[0];
		assert!(*hash_to_remove == RAW_TRANSACTION1_HASH.into()
			|| *hash_to_remove == RAW_TRANSACTION2_HASH.into()
			|| *hash_to_remove == RAW_TRANSACTION3_HASH.into()
			|| *hash_to_remove == RAW_TRANSACTION4_HASH.into());
		assert_eq!(pool.get_transactions_ids().len(), 4);

		// remove and check remaining transactions
		let removed = pool.remove_by_hash(&hash_to_remove);
		assert!(removed.is_some());
		assert_eq!(removed.unwrap().hash(), *hash_to_remove);
		assert_eq!(pool.get_transactions_ids().len(), 3);

		// remove non-existant transaction
		let nonexistant_hash: H256 = "0000000000000000000000000000000000000000000000000000000000000000".into();
		assert_eq!(pool.remove_by_hash(&nonexistant_hash), None);
		assert_eq!(pool.get_transactions_ids().len(), 3);
	}

	#[test]
	fn test_memory_pool_insert_parent_after_child() {
		let parent_transaction: Transaction = "00000000000164000000000000000000000000".into();
		let child_transaction: Transaction = "0000000001545ac9cffeaa3ee074f08a5306e703cb30883192ed9b10ee9ddb76824e4985070000000000000000000000000000".into();
		let grandchild_transaction: Transaction = "0000000001cc1d8279403880bfdd7682c28ed8441a138f96ae1dc5fd90bc928b88d48107a90000000000000000000164000000000000000000000000".into();
		let parent_transaction_hash = parent_transaction.hash();
		let child_transaction_hash = child_transaction.hash();
		let grandchild_transaction_hash = grandchild_transaction.hash();

		// insert child, then parent
		let mut pool = MemoryPool::new();
		pool.insert_verified(grandchild_transaction); // timestamp 0
		pool.insert_verified(child_transaction); // timestamp 1
		pool.insert_verified(parent_transaction); // timestamp 2

		// check that parent transaction was removed before child trnasaction
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0].hash(), parent_transaction_hash);
		assert_eq!(transactions[1].hash(), child_transaction_hash);
		assert_eq!(transactions[2].hash(), grandchild_transaction_hash);
	}

	#[test]
	fn test_memory_pool_insert_parent_before_child() {
		let parent_transaction: Transaction = "00000000000164000000000000000000000000".into();
		let child_transaction: Transaction = "0000000001545ac9cffeaa3ee074f08a5306e703cb30883192ed9b10ee9ddb76824e4985070000000000000000000164000000000000000000000000".into();
		let grandchild_transaction: Transaction = "0000000001cc1d8279403880bfdd7682c28ed8441a138f96ae1dc5fd90bc928b88d48107a90000000000000000000164000000000000000000000000".into();
		let parent_transaction_hash = parent_transaction.hash();
		let child_transaction_hash = child_transaction.hash();
		let grandchild_transaction_hash = grandchild_transaction.hash();

		// insert child, then parent
		let mut pool = MemoryPool::new();
		pool.insert_verified(parent_transaction); // timestamp 0
		pool.insert_verified(child_transaction); // timestamp 1
		pool.insert_verified(grandchild_transaction); // timestamp 2

		// check that parent transaction was removed before child trnasaction
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0].hash(), parent_transaction_hash);
		assert_eq!(transactions[1].hash(), child_transaction_hash);
		assert_eq!(transactions[2].hash(), grandchild_transaction_hash);
	}

	#[test]
<<<<<<< HEAD
	fn test_memory_pool_insert_child_after_remove_by_hash() {
=======
	fn test_memory_pool_transaction_dependent_transactions_insert_after_remove_by_hash() {
>>>>>>> origin/master
		let raw_parent_transaction = "00000000000164000000000000000000000000";
		let raw_child_transaction = "0000000001545ac9cffeaa3ee074f08a5306e703cb30883192ed9b10ee9ddb76824e4985070000000000000000000164000000000000000000000000";
		let raw_grandchild_transaction = "0000000001cc1d8279403880bfdd7682c28ed8441a138f96ae1dc5fd90bc928b88d48107a90000000000000000000164000000000000000000000000";
		let parent_transaction: Transaction = raw_parent_transaction.into();
		let child_transaction: Transaction = raw_child_transaction.into();
		let grandchild_transaction: Transaction = raw_grandchild_transaction.into();
		let parent_transaction_hash = parent_transaction.hash();
		let child_transaction_hash = child_transaction.hash();
		let grandchild_transaction_hash = grandchild_transaction.hash();

		// insert child, then parent
		let mut pool = MemoryPool::new();
		pool.insert_verified(parent_transaction);
		pool.insert_verified(child_transaction);
		pool.insert_verified(grandchild_transaction);

		// remove child transaction & make sure that other transactions are still there
		pool.remove_by_hash(&child_transaction_hash);
		assert_eq!(pool.get_transactions_ids().len(), 2);

		// insert child transaction back to the pool & assert transactions are removed in correct order
		pool.insert_verified(raw_child_transaction.into());
<<<<<<< HEAD
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTransactionScore);
=======
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByMinerScore);
>>>>>>> origin/master
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0].hash(), parent_transaction_hash);
		assert_eq!(transactions[1].hash(), child_transaction_hash);
		assert_eq!(transactions[2].hash(), grandchild_transaction_hash);
	}

	#[test]
	fn test_memory_pool_get_information() {
		let mut pool = construct_memory_pool();
		let pool_sizes = [901, 676, 451, 226, 0];
		let removals = [RAW_TRANSACTION1_HASH, RAW_TRANSACTION2_HASH, RAW_TRANSACTION3_HASH, RAW_TRANSACTION4_HASH];
		// check pool information after removing each transaction
		for i in 0..pool_sizes.len() {
			let expected_pool_count = 5 - i - 1;
			let expected_pool_size = pool_sizes[i];
			let info = pool.information();
			assert_eq!(info.transactions_count, expected_pool_count);
			assert_eq!(info.transactions_size_in_bytes, expected_pool_size);

			if expected_pool_size != 0 {
				pool.remove_by_hash(&removals[i].into());
			}
		}
	}

	#[test]
	fn test_memory_pool_timestamp_ordering_strategy() {
		let mut pool = construct_memory_pool();

		// remove transactions [4, 1, 2] (timestamps: [0, 0, 1])
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 3);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION4_HASH.into());
		assert_eq!(transactions[1].hash(), RAW_TRANSACTION1_HASH.into());
		assert_eq!(transactions[2].hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(pool.get_transactions_ids().len(), 1);

		// remove transactions [3] (timestamps: [2])
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByTimestamp);
		assert_eq!(transactions.len(), 1);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION3_HASH.into());
	}

	#[test]
	fn test_memory_pool_transaction_score_ordering_strategy() {
		let mut pool = construct_memory_pool();

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByTransactionScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION1_HASH.into());
		assert_eq!(transactions[1].hash(), RAW_TRANSACTION3_HASH.into());
		assert_eq!(transactions[2].hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(transactions[3].hash(), RAW_TRANSACTION4_HASH.into());
		assert_eq!(pool.get_transactions_ids().len(), 0);
	}

	#[test]
	fn test_memory_pool_transaction_score_ordering_strategy_with_virtual_fee() {
		let mut pool = construct_memory_pool();

		// increase miner score of transaction 4 to move it to position #1
		pool.set_virtual_fee(&RAW_TRANSACTION4_HASH.into(), 1000000000);
		// decrease miner score of transaction 3 to move it to position #4
		pool.set_virtual_fee(&RAW_TRANSACTION3_HASH.into(), -500000000);

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByTransactionScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION4_HASH.into());
		assert_eq!(transactions[1].hash(), RAW_TRANSACTION1_HASH.into());
		assert_eq!(transactions[2].hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(transactions[3].hash(), RAW_TRANSACTION3_HASH.into());
		assert_eq!(pool.remove_n_with_strategy(1, OrderingStrategy::ByTransactionScore).len(), 0);
	}

	#[test]
	fn test_memory_pool_package_score_ordering_strategy() {
		// sizes of all transactions are equal to 60
		// chain1:
		//   parent: fee = 30
		//   child: fee = 50
		// chain2:
		//   parent: fee = 35
		//   child: fee = 10
		//   grandchild: fee: 100
		let chain1_parent: Transaction = "00000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000011e000000000000000000000000".into();
		let chain1_child: Transaction = "0000000001160f027c10384f1c6ebfe5d4c554cebe944d3c66f4ea1c07fab2de09eb42bfb10000000000000000000132000000000000000000000000".into();
		let chain2_parent: Transaction = "000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000123000000000000000000000000".into();
		let chain2_child: Transaction = "00000000018dc8362908a6286b3fb74399346cfefe56aec955fa3438f00c7c3ce4dea963ab000000000000000000010a000000000000000000000000".into();
		let chain2_grandchild: Transaction = "00000000010448122df969ee00da3f4070915bdae9d900e32a57ba2ecfe99146e1eaf347900000000000000000000164000000000000000000000000".into();
		let chain1_parent_hash = chain1_parent.hash();
		let chain1_child_hash = chain1_child.hash();
		let chain2_parent_hash = chain2_parent.hash();
		let chain2_child_hash = chain2_child.hash();
		let chain2_grandchild_hash = chain2_grandchild.hash();

		let mut pool = MemoryPool::new();

		// compared by simple miner score:
		// score({ chain1_parent }) = 30/60
		// <
		// score({ chain2_parent }) = 35/60
		let expected = vec![chain2_parent_hash.clone(), chain1_parent_hash.clone()];
		pool.insert_verified(chain1_parent);
		pool.insert_verified(chain2_parent);
		assert_eq!(pool.read_n_with_strategy(2, OrderingStrategy::ByPackageScore), expected);

		// { chain1_parent, chain1_child } now have bigger score than { chain2_parent }:
		// score({ chain1_parent, chain1_child }) = (30 + 50) / 120 ~ 0.667
		// >
		// score({ chain2_parent }) = 35/60 ~ 0.583
		// => chain1 is boosted
		// => so transaction with lesser individual score (but with bigger package score) is mined first
		pool.insert_verified(chain1_child);
		let expected = vec![chain1_parent_hash.clone(), chain1_child_hash.clone(), chain2_parent_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// { chain1_parent, chain1_child } still have bigger score than { chain2_parent, chain2_child }
		// score({ chain1_parent, chain1_child }) = (30 + 35) / 120 ~ 0.625
		// >
		// score({ chain2_parent, chain2_child }) = (35 + 10) / 120 ~ 0.375
		// => chain2 is not boosted
		pool.insert_verified(chain2_child);
		let expected = vec![chain1_parent_hash.clone(), chain1_child_hash.clone(),
			chain2_parent_hash.clone(), chain2_child_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);

		// { chain1_parent, chain1_child } now have lesser score than { chain2_parent, chain2_child, chain2_grandchild }
		// score({ chain1_parent, chain1_child }) = (30 + 35) / 120 ~ 0.625
		// <
		// score({ chain2_parent, chain2_child, chain2_grandchild }) = (35 + 10 + 100) / 180 ~ 0.806
		// => chain2 is boosted
		pool.insert_verified(chain2_grandchild);
		let expected = vec![chain2_parent_hash.clone(), chain2_child_hash.clone(), chain2_grandchild_hash.clone(),
			chain1_parent_hash.clone(), chain1_child_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);

		// add virtual fee to the chain1_child so that chain1 is back to the position #1
		pool.set_virtual_fee(&chain1_child_hash, 500i64);
		let expected = vec![chain1_parent_hash.clone(), chain1_child_hash.clone(), chain2_parent_hash.clone(),
			chain2_child_hash.clone(), chain2_grandchild_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);
	}

	#[test]
	fn test_memory_pool_package_score_ordering_strategy_opposite_insert_order() {
		// sizes of all transactions are equal to 60
		// chain1:
		//   parent: fee = 17
		//   child: fee = 50
		//   grandchild: fee = 7
		// chain2:
		//   parent: fee = 20
		let chain1_parent: Transaction = "000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000111000000000000000000000000".into();
		let chain1_child: Transaction = "0000000001213fb22ef427d846506b3b4a1474db4c520fce1036d7150e10fb244d6809c1a10000000000000000000132000000000000000000000000".into();
		let chain1_grandchild: Transaction = "0000000001fb7c7a67c8b2e915b4dd562b151fa63098184dc310c37c36a8ea56f81c6e645a0000000000000000000107000000000000000000000000".into();
		let chain2_parent: Transaction = "000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000114000000000000000000000000".into();
		let chain1_parent_hash = chain1_parent.hash();
		let chain1_child_hash = chain1_child.hash();
		let chain1_grandchild_hash = chain1_grandchild.hash();
		let chain2_parent_hash = chain2_parent.hash();

		let mut pool = MemoryPool::new();

		// chain1_parent is not linked to the chain1_grandchild
		// => they are in separate chains now
		// => chain2 has greater score than both of these chains
		pool.insert_verified(chain2_parent);
		pool.insert_verified(chain1_parent);
		pool.insert_verified(chain1_grandchild);
		let expected = vec![chain2_parent_hash.clone(), chain1_parent_hash.clone(), chain1_grandchild_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// insert the missing transaction to link together chain1
		// => it now will have better score than chain2
		pool.insert_verified(chain1_child);
		let expected = vec![chain1_parent_hash.clone(), chain1_child_hash.clone(), chain2_parent_hash.clone(),
			chain1_grandchild_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);
	}

	#[test]
	fn test_memory_pool_complex_transactions_tree_opposite_insert_order() {
		// all transaction have equal size
		// level0 transactions:
		//   level00: fee = 10
		//   level01: fee = 20
		//   level02: fee = 30
		// level1 transactions:
		//   level00 -> level10: fee = 40
		//   level00 + level01 -> level11: fee = 50
		// level2 transactions:
		//   level02 + level10 + level11 -> level20: fee = 60
		let level00: Transaction = "0000000003000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010a000000000000000000000000".into();
		let level01: Transaction = "00000000030000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000114000000000000000000000000".into();
		let level02: Transaction = "0000000003000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000011e000000000000000000000000".into();
		let level10: Transaction = "0000000003f0c6441fcf6ec5feff2ec1d4791f3378f0ee3ca763f037465ecbde2defb7dbbe000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000128000000000000000000000000".into();
		let level11: Transaction = "0000000003f0c6441fcf6ec5feff2ec1d4791f3378f0ee3ca763f037465ecbde2defb7dbbe0000000000000000004e4ae7a98cc93a19f42df8dd0439578c4984650782160058c120d797f457a49b00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000132000000000000000000000000".into();
		let level20: Transaction = "0000000003f7028778f8f0eb8b3b7f082c07842d9c58231286626f891d1b2b1aec562f186e0000000000000000001bcdef7650948fe1203aa1995ff61dd2610ac76a6f5b26399907319d40e6ba0500000000000000000059137aa9303e3b33e82e401a529c28dece1bc6785a001c82f9a062ce69a65fee000000000000000000013c000000000000000000000000".into();
		let level00_hash = level00.hash();
		let level01_hash = level01.hash();
		let level02_hash = level02.hash();
		let level10_hash = level10.hash();
		let level11_hash = level11.hash();
		let level20_hash = level20.hash();

		let mut pool = MemoryPool::new();

		// insert level1 + level2. There are two chains:
		// score({ level10, level20 }) = 40 + 60
		// score({ level11, level20 }) = 50 + 60
		// And three transactions:
		// score(level10) = 40
		// score(level11) = 50
		// score(level20) = 60
		pool.insert_verified(level20);
		pool.insert_verified(level10);
		pool.insert_verified(level11);
		let expected = vec![level11_hash.clone(), level10_hash.clone(), level20_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(3, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Three chains:
		// score({ level10, level20 }) = 40 + 60
		// score({ level11, level20 }) = 50 + 60
		// score({ level02, level20 }) = 30 + 60
		pool.insert_verified(level02);
		let expected = vec![level11_hash.clone(), level10_hash.clone(), level02_hash.clone(), level20_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(4, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Three chains:
		// score({ level10, level20 }) = 40 + 60 / 2 = 0.5
		// score({ level01, level11, level20 }) = 20 + 50 + 60 / 3 ~ 0.333
		// score({ level02, level20 }) = 30 + 60 / 2 = 0.45
		// but second chain will be removed first anyway because previous #1 ({ level11, level20}) noew depends on level 01
		pool.insert_verified(level01);
		let expected = vec![level10_hash.clone(), level02_hash.clone(), level01_hash.clone(), level11_hash.clone(), level20_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(5, OrderingStrategy::ByPackageScore), expected);

		// insert another one transaction from the chain. Four chains:
		// score({ level00, level10, level20 }) = (10 + 40 + 60) / (60 + 60 + 142) ~ 0.420
		// score({ level00, level11, level20 }) = (10 + 50 + 60) / (60 + 60 + 142) ~ 0.458
		// score({ level10, level11, level20 }) = (20 + 50 + 60) / (60 + 60 + 142) ~ 0.496
		// score({ level02, level20 }) = (30 + 60) / (60 + 142) ~ 0.445
		pool.insert_verified(level00);
		let expected = vec![level02_hash.clone(), level01_hash.clone(), level00_hash.clone(),
			level11_hash.clone(), level10_hash.clone(), level20_hash.clone()];
		assert_eq!(pool.read_n_with_strategy(6, OrderingStrategy::ByTransactionScore), expected);
		assert_eq!(pool.read_n_with_strategy(6, OrderingStrategy::ByPackageScore), expected);
	}
}
