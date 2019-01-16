use std::collections::HashMap;
use chain::{Transaction, TransactionOutput, OutPoint};
use storage::TransactionOutputProvider;
use miner::{DoubleSpendCheckResult, HashedOutPoint, NonFinalDoubleSpendSet};
use verification::TransactionError;
use super::super::types::{MemoryPoolRef, StorageRef};

/// Transaction output observer, which looks into both storage && into memory pool.
/// It also allows to replace non-final transactions in the memory pool.
pub struct MemoryPoolTransactionOutputProvider {
	/// Storage provider
	storage_provider: StorageRef,
	/// Transaction inputs from memory pool transactions
	mempool_inputs: HashMap<HashedOutPoint, Option<TransactionOutput>>,
	/// Previous outputs, for which we should return 'Not spent' value.
	/// These are used when new version of transaction is received.
	nonfinal_spends: Option<NonFinalDoubleSpendSet>,
}

impl MemoryPoolTransactionOutputProvider {
	/// Create new provider for verifying given transaction
	pub fn for_transaction(storage: StorageRef, memory_pool: &MemoryPoolRef, transaction: &Transaction) -> Result<Self, TransactionError> {
		// we have to check if there are another in-mempool transactions which spent same outputs here
		let memory_pool = memory_pool.read();
		let check_result = memory_pool.check_double_spend(transaction);
		match check_result {
			// input of transaction is already spent by another final transaction from memory pool
			DoubleSpendCheckResult::DoubleSpend(_, hash, index) => Err(TransactionError::UsingSpentOutput(hash, index)),
			// there are no transactions, which are spending same inputs in memory pool
			DoubleSpendCheckResult::NoDoubleSpend => Ok(MemoryPoolTransactionOutputProvider {
				storage_provider: storage,
				mempool_inputs: transaction.inputs.iter()
					.map(|input| (
						input.previous_output.clone().into(),
						memory_pool.transaction_output(&input.previous_output, usize::max_value()),
					)).collect(),
				nonfinal_spends: None,
			}),
			// there are non-final transactions, which are spending same inputs in memory pool
			DoubleSpendCheckResult::NonFinalDoubleSpend(nonfinal_spends) => Ok(MemoryPoolTransactionOutputProvider {
				storage_provider: storage,
				mempool_inputs: transaction.inputs.iter()
					.map(|input| (
						input.previous_output.clone().into(),
						memory_pool.transaction_output(&input.previous_output, usize::max_value()),
					)).collect(),
				nonfinal_spends: Some(nonfinal_spends),
			}),
		}
	}
}

impl TransactionOutputProvider for MemoryPoolTransactionOutputProvider {
	fn transaction_output(&self, prevout: &OutPoint, transaction_index: usize) -> Option<TransactionOutput> {
		let hashed_prevout: HashedOutPoint = prevout.clone().into();

		// check if that is output of some transaction, which is vitually removed from memory pool
		if let Some(ref nonfinal_spends) = self.nonfinal_spends {
			if nonfinal_spends.dependent_spends.contains(&hashed_prevout) {
				// transaction is trying to replace some nonfinal transaction
				// + it is also depends on this transaction
				// => this is definitely an error
				return None;
			}
		}

		// check if this is output from memory pool transaction
		if let Some(output) = self.mempool_inputs.get(&hashed_prevout) {
			if let Some(ref output) = *output {
				return Some(output.clone());
			}
		}

		// now check in storage
		self.storage_provider.transaction_output(prevout, transaction_index)
	}

	fn is_spent(&self, prevout: &OutPoint) -> bool {
		// check if this output is spent by some non-final mempool transaction
		if let Some(ref nonfinal_spends) = self.nonfinal_spends {
			if nonfinal_spends.double_spends.contains(&prevout.clone().into()) {
				return false;
			}
		}

		// we can omit memory_pool check here, because it has been completed in `for_transaction` method
		// => just check spending in storage
		self.storage_provider.is_spent(prevout)
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use parking_lot::RwLock;
	use chain::OutPoint;
	use storage::TransactionOutputProvider;
	use db::BlockChainDatabase;
	use miner::{MemoryPool, NonZeroFeeCalculator};
	use super::MemoryPoolTransactionOutputProvider;

	#[test]
	fn when_transaction_depends_on_removed_nonfinal_transaction() {
		let dchain = &mut test_data::ChainBuilder::new();

		test_data::TransactionBuilder::with_output(10).store(dchain)					// t0
			.reset().set_input(&dchain.at(0), 0).add_output(20).lock().store(dchain)	// nonfinal: t0[0] -> t1
			.reset().set_input(&dchain.at(1), 0).add_output(30).store(dchain)			// dependent: t0[0] -> t1[0] -> t2
			.reset().set_input(&dchain.at(0), 0).add_output(40).store(dchain);			// good replacement: t0[0] -> t3

		let storage = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let memory_pool = Arc::new(RwLock::new(MemoryPool::new()));
		{
			memory_pool.write().insert_verified(dchain.at(0).into(), &NonZeroFeeCalculator);
			memory_pool.write().insert_verified(dchain.at(1).into(), &NonZeroFeeCalculator);
			memory_pool.write().insert_verified(dchain.at(2).into(), &NonZeroFeeCalculator);
		}

		// when inserting t3:
		// check that is_spent(t0[0]) == Some(false) (as it is spent by nonfinal t1)
		// check that is_spent(t1[0]) == None (as t1 is virtually removed)
		// check that is_spent(t2[0]) == None (as t2 is virtually removed)
		// check that previous_transaction_output(t0[0]) = Some(_)
		// check that previous_transaction_output(t1[0]) = None (as t1 is virtually removed)
		// check that previous_transaction_output(t2[0]) = None (as t2 is virtually removed)
		// =>
		// if t3 is also depending on t1[0] || t2[0], it will be rejected by verification as missing inputs
		let provider = MemoryPoolTransactionOutputProvider::for_transaction(storage, &memory_pool, &dchain.at(3)).unwrap();
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(0).hash(), index: 0, }), false);
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(1).hash(), index: 0, }), false);
		assert_eq!(provider.is_spent(&OutPoint { hash: dchain.at(2).hash(), index: 0, }), false);
		assert_eq!(provider.transaction_output(&OutPoint { hash: dchain.at(0).hash(), index: 0, }, 0), Some(dchain.at(0).outputs[0].clone()));
		assert_eq!(provider.transaction_output(&OutPoint { hash: dchain.at(1).hash(), index: 0, }, 0), None);
		assert_eq!(provider.transaction_output(&OutPoint { hash: dchain.at(2).hash(), index: 0, }, 0), None);
	}
}
