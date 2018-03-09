use chain::Transaction;
use ser::Serializable;
use storage::TransactionProvider;

pub fn transaction_fee(store: &TransactionProvider, transaction: &Transaction) -> u64 {
	let inputs_sum = transaction.inputs.iter().map(|input| {
		let input_transaction = store.transaction(&input.previous_output.hash)
			.expect("transaction must be verified by caller");
		input_transaction.outputs[input.previous_output.index as usize].value
	}).sum::<u64>();
	let outputs_sum = transaction.outputs.iter().map(|output| output.value).sum();
	inputs_sum.saturating_sub(outputs_sum)
}

pub fn transaction_fee_rate(store: &TransactionProvider, transaction: &Transaction) -> u64 {
	transaction_fee(store, transaction) / transaction.serialized_size() as u64
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

		assert_eq!(transaction_fee(db.as_transaction_provider(), &tx0), 0);
		assert_eq!(transaction_fee(db.as_transaction_provider(), &tx2), 500_000);

		assert_eq!(transaction_fee_rate(db.as_transaction_provider(), &tx0), 0);
		assert_eq!(transaction_fee_rate(db.as_transaction_provider(), &tx2), 4_901);
	}
}
