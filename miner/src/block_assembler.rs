use primitives::hash::H256;
use db::{SharedStore, IndexedTransaction};
use memory_pool::{MemoryPool, OrderingStrategy};

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
	pub coinbase_value: u32,
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
pub struct BlockAssembler;

impl BlockAssembler {
	pub fn create_new_block(store: &SharedStore, mempool: &MemoryPool, time: u32) -> BlockTemplate {
		// get best block
		// take it's hash && height
		let best_block = store.best_block().expect("Cannot assemble new block without genesis block");
		let previous_header_hash = best_block.hash;
		let height = best_block.number + 1;

		// TODO: calculate nbits (retarget may be required)
		let nbits = 0;

		// TODO: calculate version
		let version = 0;

		// TODO: use constants and real values
		let mut block_size = SizePolicy::new(0, 1_000_000, 100_000, 50);
		// TODO: use constants and real values
		let mut sigops = SizePolicy::new(0, 2000, 8, 50);
		// TODO: calculate coinbase fee
		let mut coinbase_value = 0u32;

		let mut transactions = Vec::new();
		// add priority transactions
		BlockAssembler::fill_transactions(mempool, &mut block_size, &mut sigops, &mut coinbase_value, &mut transactions, OrderingStrategy::ByTransactionScore);
		// add package transactions
		BlockAssembler::fill_transactions(mempool, &mut block_size, &mut sigops, &mut coinbase_value, &mut transactions, OrderingStrategy::ByPackageScore);

		BlockTemplate {
			version: version,
			previous_header_hash: previous_header_hash,
			time: time,
			nbits: nbits,
			height: height,
			transactions: transactions,
			coinbase_value: coinbase_value,
		}
	}

	fn fill_transactions(
		mempool: &MemoryPool,
		block_size: &mut SizePolicy,
		sigops: &mut SizePolicy,
		coinbase_value: &mut u32,
		transactions: &mut Vec<IndexedTransaction>,
		strategy: OrderingStrategy
	) {
		for entry in mempool.iter(strategy) {
			if transactions.iter().any(|x| x.hash == entry.hash) {
				break;
			}

			// TODO: calucalte sigops
			let transaction_size = entry.size as u32;
			let transaction_sigops = 0;

			let size_step = block_size.decide(transaction_size);
			let sigops_step = sigops.decide(transaction_sigops);

			let transaction = IndexedTransaction {
				transaction: entry.transaction.clone(),
				hash: entry.hash.clone(),
			};

			match size_step.and(sigops_step) {
				NextStep::Append => {
					// miner_fee is i64, but we can safely cast it to u32
					// memory pool should restrict miner fee to be positive
					*coinbase_value += entry.miner_fee as u32;
					transactions.push(transaction);
				},
				NextStep::FinishAndAppend => {
					transactions.push(transaction);
					break;
				},
				NextStep::Ignore => (),
				NextStep::FinishAndIgnore => {
					break;
				},
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{SizePolicy, NextStep};

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
}
