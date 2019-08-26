use chain::Transaction;
use ser::Serializable;
use storage::{TransactionOutputProvider, DuplexTransactionOutputProvider};
use MemoryPool;

/// Transaction fee calculator for memory pool
pub trait MemoryPoolFeeCalculator {
	/// Compute transaction fee
	fn calculate(&self, memory_pool: &MemoryPool, tx: &Transaction) -> u64;
}

/// Fee calculator that computes sum of real transparent fee + real shielded fee.
pub struct FeeCalculator<'a>(pub &'a dyn TransactionOutputProvider);

impl<'a> MemoryPoolFeeCalculator for FeeCalculator<'a> {
	fn calculate(&self, memory_pool: &MemoryPool, tx: &Transaction) -> u64 {
		let tx_out_provider = DuplexTransactionOutputProvider::new(self.0, memory_pool);
		transaction_fee(&tx_out_provider, tx)
	}
}

/// Used in tests in this && external crates
#[cfg(any(test, feature = "test-helpers"))]
pub struct NonZeroFeeCalculator;

#[cfg(any(test, feature = "test-helpers"))]
impl MemoryPoolFeeCalculator for NonZeroFeeCalculator {
	fn calculate(&self, _: &MemoryPool, tx: &Transaction) -> u64 {
		// add 100_000_000 to make sure tx won't be rejected by txpoool because of fee
		// + but keep ordering by outputs sum
		100_000_000 + tx.outputs.iter().fold(0, |acc, output| acc + output.value)
	}
}

pub fn transaction_fee(store: &dyn TransactionOutputProvider, tx: &Transaction) -> u64 {
	let input_value = tx.inputs.iter().fold(0, |acc, input| acc + store
		.transaction_output(&input.previous_output, ::std::usize::MAX)
		.map(|output| output.value)
		.unwrap_or(0));
	let output_value = tx.outputs.iter().fold(0, |acc, output| acc + output.value);

	input_value.saturating_sub(output_value)
}

pub fn transaction_fee_rate(store: &dyn TransactionOutputProvider, tx: &Transaction) -> u64 {
	transaction_fee(store, tx) / tx.serialized_size() as u64
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use storage::{AsSubstore};
	use db::BlockChainDatabase;
	use super::*;

	#[test]
	fn test_transaction_fee() {
		let b0 = test_data::block_builder().header().nonce(1).build()
			.transaction()
				.output().value(1_000_000).build()
				.output().value(2_000_000).build()
				.build()
			.build();
		let tx0 = b0.transactions[0].clone();
		let tx0_hash = tx0.hash();
		let b1 = test_data::block_builder().header().parent(b0.hash().clone()).nonce(2).build()
			.transaction()
				.input().hash(tx0_hash.clone()).index(0).build()
				.input().hash(tx0_hash).index(1).build()
				.output().value(2_500_000).build()
				.build()
			.build();
		let tx2 = b1.transactions[0].clone();

		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![b0.into(), b1.into()]));

		assert_eq!(transaction_fee(db.as_transaction_output_provider(), &tx0), 0);
		assert_eq!(transaction_fee(db.as_transaction_output_provider(), &tx2), 500_000);

		assert_eq!(transaction_fee_rate(db.as_transaction_output_provider(), &tx0), 0);
		assert_eq!(transaction_fee_rate(db.as_transaction_output_provider(), &tx2), 4_901);
	}
}
