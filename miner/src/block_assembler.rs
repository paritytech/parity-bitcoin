use primitives::hash::H256;
use chain::{OutPoint, TransactionOutput};
use db::{SharedStore, IndexedTransaction, PreviousTransactionOutputProvider};
use network::Magic;
use memory_pool::{MemoryPool, OrderingStrategy, Entry};
use verification::{work_required, block_reward_satoshi, transaction_sigops};
pub use verification::constants::{MAX_BLOCK_SIZE, MAX_BLOCK_SIGOPS};

const BLOCK_VERSION: u32 = 0x20000000;
const BLOCK_HEADER_SIZE: u32 = 4 + 32 + 32 + 4 + 4 + 4;

/// Block template as described in BIP0022
/// Minimal version
/// [BIP0022](https://github.com/bitcoin/bips/blob/master/bip-0022.mediawiki#block-template-request)
pub struct BlockTemplate {
	/// Version
	pub version: u32,
	/// The hash of previous block
	pub previous_header_hash: H256,
	/// The current time as seen by the server
	pub time: u32,
	/// The compressed difficulty
	pub nbits: u32,
	/// Block height
	pub height: u32,
	/// Block transactions (excluding coinbase)
	pub transactions: Vec<IndexedTransaction>,
	/// Total funds available for the coinbase (in Satoshis)
	pub coinbase_value: u64,
	/// Number of bytes allowed in the block
	pub size_limit: u32,
	/// Number of sigops allowed in the block
	pub sigop_limit: u32,
}

/// Block size and number of signatures opcodes is limited
/// This structure should be used for storing this values.
struct SizePolicy {
	/// Current size
	current_size: u32,
	/// Max size
	max_size: u32,
	/// When current_size + size_buffer > max_size
	/// we need to start finishing the block
	size_buffer: u32,
	/// Number of transactions checked since finishing started
	finish_counter: u32,
	/// Number of transactions to check when finishing the block
	finish_limit: u32,
}

/// When appending transaction, opcode count and block size policies
/// must agree on appending the transaction to the block
#[derive(Debug, PartialEq, Copy, Clone)]
enum NextStep {
	/// Append the transaction, check the next one
	Append,
	/// Append the transaction, do not check the next one
	FinishAndAppend,
	/// Ignore transaction, check the next one
	Ignore,
	/// Ignore transaction, do not check the next one
	FinishAndIgnore,
}

impl NextStep {
	fn and(self, other: NextStep) -> Self {
		match (self, other) {
			(_, NextStep::FinishAndIgnore) |
			(NextStep::FinishAndIgnore, _) |
			(NextStep::FinishAndAppend, NextStep::Ignore) |
			(NextStep::Ignore, NextStep::FinishAndAppend) => NextStep::FinishAndIgnore,

			(NextStep::Ignore, _) |
			(_, NextStep::Ignore) => NextStep::Ignore,

			(_, NextStep::FinishAndAppend) |
			(NextStep::FinishAndAppend, _) => NextStep::FinishAndAppend,

			(NextStep::Append, NextStep::Append) => NextStep::Append,
		}
	}
}

impl SizePolicy {
	fn new(current_size: u32, max_size: u32, size_buffer: u32, finish_limit: u32) -> Self {
		SizePolicy {
			current_size: current_size,
			max_size: max_size,
			size_buffer: size_buffer,
			finish_counter: 0,
			finish_limit: finish_limit,
		}
	}

	fn decide(&mut self, size: u32) -> NextStep {
		let finishing = self.current_size + self.size_buffer > self.max_size;
		let fits = self.current_size + size <= self.max_size;
		let finish = self.finish_counter + 1 >= self.finish_limit;

		if finishing {
			self.finish_counter += 1;
		}

		if fits {
			self.current_size += size;
		}

		match (fits, finish) {
			(true, true) => NextStep::FinishAndAppend,
			(true, false) => NextStep::Append,
			(false, true) => NextStep::FinishAndIgnore,
			(false, false) => NextStep::Ignore,
		}
	}
}

/// Block assembler
pub struct BlockAssembler {
	pub max_block_size: u32,
	pub max_block_sigops: u32,
}

impl Default for BlockAssembler {
	fn default() -> Self {
		BlockAssembler {
			max_block_size: MAX_BLOCK_SIZE as u32,
			max_block_sigops: MAX_BLOCK_SIGOPS as u32,
		}
	}
}

/// Iterator iterating over mempool transactions and yielding only those which fit the block
struct FittingTransactionsIterator<'a, T> {
	/// Shared store is used to query previous transaction outputs from database
	store: &'a PreviousTransactionOutputProvider,
	/// Memory pool transactions iterator
	iter: T,
	/// Size policy decides if transactions size fits the block
	block_size: SizePolicy,
	/// Sigops policy decides if transactions sigops fits the block
	sigops: SizePolicy,
	/// Previous entries are needed to get previous transaction outputs
	previous_entries: Vec<&'a Entry>,
	/// True if block is already full
	finished: bool,
}

impl<'a, T> FittingTransactionsIterator<'a, T> where T: Iterator<Item = &'a Entry> {
	fn new(store: &'a PreviousTransactionOutputProvider, iter: T, max_block_size: u32, max_block_sigops: u32) -> Self {
		FittingTransactionsIterator {
			store: store,
			iter: iter,
			// reserve some space for header and transations len field
			block_size: SizePolicy::new(BLOCK_HEADER_SIZE + 4, max_block_size, 1_000, 50),
			sigops: SizePolicy::new(0, max_block_sigops, 8, 50),
			previous_entries: Vec::new(),
			finished: false,
		}
	}
}

impl<'a, T> PreviousTransactionOutputProvider for FittingTransactionsIterator<'a, T> where T: Send + Sync {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.store.previous_transaction_output(prevout)
			.or_else(|| {
				self.previous_entries.iter()
					.find(|e| e.hash == prevout.hash)
					.and_then(|e| e.transaction.outputs.iter().nth(prevout.index as usize))
					.cloned()
			})
	}
}

impl<'a, T> Iterator for FittingTransactionsIterator<'a, T> where T: Iterator<Item = &'a Entry> + Send + Sync {
	type Item = &'a Entry;

	fn next(&mut self) -> Option<Self::Item> {
		while !self.finished {
			let entry = match self.iter.next() {
				Some(entry) => entry,
				None => {
					self.finished = true;
					return None;
				}
			};

			let transaction_size = entry.size as u32;
			let bip16_active = true;
			let sigops_count = transaction_sigops(&entry.transaction, self, bip16_active) as u32;

			let size_step = self.block_size.decide(transaction_size);
			let sigops_step = self.sigops.decide(sigops_count);

			match size_step.and(sigops_step) {
				NextStep::Append => {
					self.previous_entries.push(entry);
					return Some(entry);
				},
				NextStep::FinishAndAppend => {
					self.finished = true;
					self.previous_entries.push(entry);
					return Some(entry);
				},
				NextStep::Ignore => (),
				NextStep::FinishAndIgnore => {
					self.finished = true;
				},
			}
		}

		None
	}
}

impl BlockAssembler {
	pub fn create_new_block(&self, store: &SharedStore, mempool: &MemoryPool, time: u32, network: Magic) -> BlockTemplate {
		// get best block
		// take it's hash && height
		let best_block = store.best_block().expect("Cannot assemble new block without genesis block");
		let previous_header_hash = best_block.hash;
		let height = best_block.number + 1;
		let nbits = work_required(previous_header_hash.clone(), time, height, store.as_block_header_provider(), network);
		let version = BLOCK_VERSION;

		let mut coinbase_value = block_reward_satoshi(height);
		let mut transactions = Vec::new();

		let mempool_iter = mempool.iter(OrderingStrategy::ByTransactionScore);
		let tx_iter = FittingTransactionsIterator::new(store.as_previous_transaction_output_provider(), mempool_iter, self.max_block_size, self.max_block_sigops);
		for entry in tx_iter {
			// miner_fee is i64, but we can safely cast it to u64
			// memory pool should restrict miner fee to be positive
			coinbase_value += entry.miner_fee as u64;
			let tx = IndexedTransaction::new(entry.hash.clone(), entry.transaction.clone());
			transactions.push(tx);
		}

		BlockTemplate {
			version: version,
			previous_header_hash: previous_header_hash,
			time: time,
			nbits: nbits.into(),
			height: height,
			transactions: transactions,
			coinbase_value: coinbase_value,
			size_limit: self.max_block_size,
			sigop_limit: self.max_block_sigops,
		}
	}
}

#[cfg(test)]
mod tests {
	use db::IndexedTransaction;
	use verification::constants::{MAX_BLOCK_SIZE, MAX_BLOCK_SIGOPS};
	use memory_pool::Entry;
	use super::{SizePolicy, NextStep, FittingTransactionsIterator};

	#[test]
	fn test_size_policy() {
		let mut size_policy = SizePolicy::new(0, 1000, 200, 3);
		assert_eq!(size_policy.decide(100), NextStep::Append);
		assert_eq!(size_policy.decide(500), NextStep::Append);
		assert_eq!(size_policy.decide(600), NextStep::Ignore);
		assert_eq!(size_policy.decide(200), NextStep::Append);
		assert_eq!(size_policy.decide(300), NextStep::Ignore);
		assert_eq!(size_policy.decide(300), NextStep::Ignore);
		// this transaction will make counter + buffer > max size
		assert_eq!(size_policy.decide(1), NextStep::Append);
		// so now only 3 more transactions may accepted / ignored
		assert_eq!(size_policy.decide(1), NextStep::Append);
		assert_eq!(size_policy.decide(1000), NextStep::Ignore);
		assert_eq!(size_policy.decide(1), NextStep::FinishAndAppend);
		// we should not call decide again after it returned finish...
		// but we can, let's check if result is ok
		assert_eq!(size_policy.decide(1000), NextStep::FinishAndIgnore);
	}

	#[test]
	fn test_next_step_and() {
		assert_eq!(NextStep::Append.and(NextStep::Append), NextStep::Append);
		assert_eq!(NextStep::Ignore.and(NextStep::Append), NextStep::Ignore);
		assert_eq!(NextStep::FinishAndIgnore.and(NextStep::Append), NextStep::FinishAndIgnore);
		assert_eq!(NextStep::Ignore.and(NextStep::FinishAndIgnore), NextStep::FinishAndIgnore);
		assert_eq!(NextStep::FinishAndAppend.and(NextStep::FinishAndIgnore), NextStep::FinishAndIgnore);
		assert_eq!(NextStep::FinishAndAppend.and(NextStep::Ignore), NextStep::FinishAndIgnore);
		assert_eq!(NextStep::FinishAndAppend.and(NextStep::Append), NextStep::FinishAndAppend);
	}

	#[test]
	fn test_fitting_transactions_iterator_no_transactions() {
		let store: Vec<IndexedTransaction> = Vec::new();
		let entries: Vec<Entry> = Vec::new();
		let store_ref: &[_] = &store;

		let iter = FittingTransactionsIterator::new(&store_ref, entries.iter(), MAX_BLOCK_SIZE as u32, MAX_BLOCK_SIGOPS as u32);
		assert!(iter.collect::<Vec<_>>().is_empty());
	}

	#[test]
	fn test_fitting_transactions_iterator_max_block_size_reached() {
	}
}
