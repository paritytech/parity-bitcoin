//! Transactions memory pool
//!
//! `MemoryPool` keeps track of all transactions seen by the node (received from other peers) and own transactions
//! and orders them by given strategies. It works like multi-indexed priority queue, giving option to pop 'top'
//! transactions.
//! It also guarantees that ancestor-descendant relation won't break during ordered removal (ancestors always removed
//! before descendants). Removal using remove_by_hash can break this rule.
use std::rc::Rc;
use hash::H256;
use chain::Transaction;
use std::collections::HashMap;
use std::collections::HashSet;
use ser::Serializable;

/// Transactions ordering strategy
#[derive(Debug)]
pub enum OrderingStrategy {
	/// Order transactions by their timestamp
	ByTimestamp,
	/// Order transactions by miner score
	ByMinerScore,
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
	/// Transaction ancestors hashes
	ancestors: Rc<HashSet<H256>>,
	/// Transaction hash (stored for effeciency)
	hash: H256,
	/// Transaction size (stored for effeciency)
	size: usize,
	/// 'Timestamp' when transaction has entered memory pool
	timestamp: u64,
	/// Transaction fee (stored for efficiency)
	miner_fee: i64,
	/// Virtual transaction fee (a way to prioritize/penalize transaction)
	miner_virtual_fee: i64,
}

/// Multi-index transactions storage
#[derive(Debug)]
struct Storage {
	/// Transactions counter (for timestamp ordering)
	counter: u64,
	/// Total transactions size (when serialized) in bytes
	transactions_size_in_bytes: usize,
	/// By-input storage
	by_input: HashMap<H256, HashSet<H256>>,
	/// By-hash storage
	by_hash: HashMap<H256, Entry>,
	/// By-entry-time storage
	by_timestamp: timestamp_strategy::Storage,
	/// By-score storage
	by_miner_score: miner_score_strategy::Storage,
}

macro_rules! ordering_strategy {
	($strategy: ident; $($member: ident: $member_type: ty), *; $comparer: expr) => {
		mod $strategy {
			use std::rc::Rc;
			use std::cmp::Ordering;
			use hash::H256;
			use std::collections::HashSet;
			use std::collections::BTreeSet;
			use super::Entry;

			/// Lightweight struct maintain transactions ordering
			#[derive(Debug, Eq, PartialEq)]
			pub struct OrderedEntry {
				/// Transaction hash
				hash: H256,
				/// Transaction ancestors
				ancestors: Rc<HashSet<H256>>,
				/// Transaction data
				$($member: $member_type), *
			}

			impl OrderedEntry {
				pub fn for_entry(entry: &Entry) -> OrderedEntry {
					OrderedEntry {
						hash: entry.hash.clone(),
						ancestors: entry.ancestors.clone(),
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
					if self.ancestors.contains(&other.hash) {
						return Ordering::Greater
					}
					if other.ancestors.contains(&self.hash) {
						return Ordering::Less
					}

					let order = $comparer(&self, other);
					if order != Ordering::Equal {
						return order
					}

					self.hash.cmp(&other.hash)
				}
			}

			/// By-timestamp ordering storage
			#[derive(Debug)]
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
				pub fn remove(&mut self, entry: &Entry) {
					self.data.remove(&OrderedEntry::for_entry(entry));
				}

				/// An iterator that takes first n entries using predicate
				pub fn take(&self, n: usize) -> Vec<H256> {
					self.data.iter()
						.map(|ref entry| entry.hash.clone())
						.take(n)
						.collect()
				}
			}
		}
	}
}

ordering_strategy!(timestamp_strategy;
	timestamp: u64;
	|me: &Self, other: &Self|
		me.timestamp.cmp(&other.timestamp));
ordering_strategy!(miner_score_strategy;
	size: usize, miner_fee: i64, miner_virtual_fee: i64;
	|me: &Self, other: &Self| {
		// lesser miner score means later removal
		let left = (me.miner_fee + me.miner_virtual_fee) * (other.size as i64);
		let right = (other.miner_fee + other.miner_virtual_fee) * (me.size as i64);
		right.cmp(&left)
	});

macro_rules! insert_to_orderings {
	($me: expr, $entry: expr) => (
		$me.by_timestamp.insert(&$entry);
		$me.by_miner_score.insert(&$entry);
	)
}

macro_rules! remove_from_orderings {
	($me: expr, $entry: expr) => (
		$me.by_timestamp.remove(&$entry);
		$me.by_miner_score.remove(&$entry);
	)
}

impl Storage {
	pub fn new() -> Self {
		Storage {
			counter: 0,
			transactions_size_in_bytes: 0,
			by_input: HashMap::new(),
			by_hash: HashMap::new(),
			by_timestamp: timestamp_strategy::Storage::new(),
			by_miner_score: miner_score_strategy::Storage::new(),
		}
	}

	pub fn insert(&mut self, entry: Entry) {
		self.transactions_size_in_bytes += entry.size;

		self.update_by_input_on_insert(&entry);
		self.update_descendants_on_insert(&entry);

		insert_to_orderings!(self, entry);
		self.by_hash.insert(entry.hash.clone(), entry);
	}

	pub fn remove(&mut self, entry: &Entry) -> Option<Entry> {
		self.transactions_size_in_bytes -= entry.size;

		self.update_descendants_on_remove(&entry);
		self.update_by_input_on_remove(&entry);

		remove_from_orderings!(self, entry);
		self.by_hash.remove(&entry.hash)
	}

	pub fn get_by_hash(&self, h: &H256) -> Option<&Entry> {
		self.by_hash.get(h)
	}

	pub fn contains(&self, hash: &H256) -> bool {
		self.by_hash.contains_key(hash)
	}

	pub fn remove_by_hash(&mut self, h: &H256) -> Option<Entry> {
		self.by_hash.remove(h)
			.map(|entry| {
				self.remove(&entry);
				entry
			})
	}

	pub fn remove_n_with_strategy(&mut self, n: usize, strategy: OrderingStrategy) -> Vec<Transaction> {
		let hashes = {
			match strategy {
				OrderingStrategy::ByTimestamp => self.by_timestamp.take(n),
				OrderingStrategy::ByMinerScore => self.by_miner_score.take(n),
			}
		};
		hashes
			.iter()
			.map(|ref hash| {
				let entry = self.remove_by_hash(&hash)
					.expect("`hash` is read from by_* index; all hashes from indexes are also stored in `by_hash`; remove returns entry from `by_hash`; qed");
				entry.transaction
			})
			.collect()
	}

	pub fn set_virtual_fee(&mut self, h: &H256, virtual_fee: i64) {
		if let Some(ref mut entry) = self.by_hash.get_mut(h) {
			self.by_miner_score.remove(&entry);
			entry.miner_virtual_fee = virtual_fee;
			self.by_miner_score.insert(&entry);
		}
	}

	pub fn get_transactions_ids(&self) -> Vec<H256> {
		self.by_hash.keys().map(|h| h.clone()).collect()
	}

	fn update_by_input_on_insert(&mut self, entry: &Entry) {
		// maintain map { ancestor: list<descendants> } to support inserting
		// acendants & descendants in random order
		for input_hash in entry.transaction.inputs.iter().map(|input| &input.previous_output.hash) {
			self.by_input.entry(input_hash.clone()).or_insert_with(|| HashSet::new()).insert(entry.hash.clone());
		}
	}

	fn update_descendants_on_insert(&mut self, entry: &Entry) {
		// this code will run only when ancestor transaction is inserted
		// in memory pool after its descendants
		if let Some(descendants) = self.by_input.get(&entry.hash) {
			for descendant in descendants.iter() {
				if let Some(mut descendant_entry) = self.by_hash.remove(descendant) {
					{
						remove_from_orderings!(self, descendant_entry);

						let ancestors = Rc::make_mut(&mut descendant_entry.ancestors);
						ancestors.insert(entry.hash.clone());
					}
					insert_to_orderings!(self, descendant_entry);
					self.by_hash.insert(descendant_entry.hash.clone(), descendant_entry);
				};
			}
		}
	}

	fn update_by_input_on_remove(&mut self, entry: &Entry) {
		for input_hash in entry.transaction.inputs.iter().map(|input| &input.previous_output.hash) {
			let remove_entry = {
				let by_input_item = self.by_input.get_mut(input_hash)
					.expect("`entry.transaction` is immutable; on insert all transaction' inputs are stored in `by_input`; `by_input` only modified here; qed");
				by_input_item.remove(&entry.hash);
				by_input_item.is_empty()
			};
			if remove_entry {
				self.by_input.remove(input_hash);
			}
		}
	}

	fn update_descendants_on_remove(&mut self, entry: &Entry) {
		// this code will run when there still are dependent transactions in the pool
		if let Some(descendants) = self.by_input.get(&entry.hash) {
			for descendant in descendants.iter() {
				if let Some(mut descendant_entry) = self.by_hash.remove(descendant) {
					{
						remove_from_orderings!(self, descendant_entry);

						let ancestors = Rc::make_mut(&mut descendant_entry.ancestors);
						ancestors.remove(&entry.hash);
					}
					insert_to_orderings!(self, descendant_entry);
					self.by_hash.insert(descendant_entry.hash.clone(), descendant_entry);
				};
			}
		}
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
		self.storage.insert(entry);
	}

	/// Removes single transaction by its hash.
	/// All descedants remain in the pool.
	pub fn remove_by_hash(&mut self, h: &H256) -> Option<Transaction> {
		self.storage.remove_by_hash(h).map(|entry| entry.transaction)
	}

	/// Removes up to n transactions the `MemoryPool`, using selected strategy.
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
		let ancestors = Rc::new(self.get_ancestors(&t));
		let size = self.get_transaction_size(&t);
		let timestamp = self.get_timestamp();
		let miner_fee = self.get_transaction_miner_fee(&t);
		Entry {
			transaction: t,
			hash: hash,
			ancestors: ancestors,
			size: size,
			timestamp: timestamp,
			miner_fee: miner_fee,
			miner_virtual_fee: 0,
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
	fn get_timestamp(&mut self) -> u64 {
		self.storage.counter += 1;
		self.storage.counter
	}

	#[cfg(test)]
	fn get_timestamp(&self) -> u64 {
		(self.storage.by_hash.len() % 3usize) as u64
	}
}

#[cfg(test)]
mod tests {
	use std::cmp::Ordering;
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
	}

	#[test]
	fn test_memory_pool_transaction_dependent_transactions_parent_after_child() {
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
	fn test_memory_pool_transaction_dependent_transactions_parent_before_child() {
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
	fn test_memory_pool_transaction_dependent_transactions_insert_after_remove_by_hash() {
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
		let transactions = pool.remove_n_with_strategy(3, OrderingStrategy::ByMinerScore);
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
	fn test_memory_pool_miner_score_ordering_strategy() {
		let mut pool = construct_memory_pool();

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByMinerScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION1_HASH.into());
		assert_eq!(transactions[1].hash(), RAW_TRANSACTION3_HASH.into());
		assert_eq!(transactions[2].hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(transactions[3].hash(), RAW_TRANSACTION4_HASH.into());
		assert_eq!(pool.get_transactions_ids().len(), 0);
	}

	#[test]
	fn test_memory_pool_miner_score_ordering_strategy_with_virtual_fee() {
		let mut pool = construct_memory_pool();

		// increase miner score of transaction 4 to move it to position #1
		pool.set_virtual_fee(&RAW_TRANSACTION4_HASH.into(), 1000000000);
		// decrease miner score of transaction 3 to move it to position #4
		pool.set_virtual_fee(&RAW_TRANSACTION3_HASH.into(), -500000000);

		let transactions = pool.remove_n_with_strategy(4, OrderingStrategy::ByMinerScore);
		assert_eq!(transactions.len(), 4);
		assert_eq!(transactions[0].hash(), RAW_TRANSACTION4_HASH.into());
		assert_eq!(transactions[1].hash(), RAW_TRANSACTION1_HASH.into());
		assert_eq!(transactions[2].hash(), RAW_TRANSACTION2_HASH.into());
		assert_eq!(transactions[3].hash(), RAW_TRANSACTION3_HASH.into());
		assert_eq!(pool.remove_n_with_strategy(1, OrderingStrategy::ByMinerScore).len(), 0);
	}
}
