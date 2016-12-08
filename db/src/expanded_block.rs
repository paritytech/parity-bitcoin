use chain;

use primitives::H256;
use indexed_block::IndexedBlock;
use transaction_meta::TransactionMeta;
use super::Store;

pub struct TransactionData {
	// parent transaction for each input
	parents: Vec<Option<chain::Transaction>>,
	// meta of parent transaction for each input
	meta: Vec<Option<TransactionMeta>>,
}

impl TransactionData {
	fn collect(transaction: &chain::Transaction, store: &Store) -> TransactionData {
		let inputs = transaction.inputs.len();

		let mut data = TransactionData {
			parents: Vec::with_capacity(inputs),
			meta: Vec::with_capacity(inputs),
		};

		for input in &transaction.inputs {
			data.meta.push(store.transaction_meta(&input.previous_output.hash));
			data.parents.push(store.transaction(&input.previous_output.hash));
		}

		data
	}
}

pub struct ExpandedBlock {
	block: IndexedBlock,
	// guaranteed to be the same length as block.transactions()
	refs: Vec<TransactionData>,
}

impl ExpandedBlock {
	pub fn new(block: IndexedBlock, store: &Store) -> Self {
		let mut result = ExpandedBlock {
			block: block,
			refs: Vec::new(),
		};

		for (_, tx) in result.block.transactions().skip(1) {
			result.refs.push(TransactionData::collect(tx, store));
		}

		result
	}
}

pub struct ExpandedInput<'a> {
	input: &'a chain::TransactionInput,
	parent: &'a Option<chain::Transaction>,
	meta: &'a Option<TransactionMeta>,
}

pub struct ExpandedInputs<'a> {
	transaction: &'a ExpandedTransaction<'a>,
	position: usize,
}

impl<'a> Iterator for ExpandedInputs<'a> {
	type Item = ExpandedInput<'a>;

	fn next(&mut self) -> Option<ExpandedInput<'a>> {
		if self.position >= self.transaction.transaction.inputs.len() {
			None
		}
		else {
			Some(ExpandedInput {
				input: &self.transaction.transaction.inputs[self.position],
				parent: &self.transaction.refs.parents[self.position],
				meta: &self.transaction.refs.meta[self.position],
			})
		}
	}
}

pub struct ExpandedTransaction<'a> {
	transaction: &'a chain::Transaction,
	refs: &'a TransactionData,
}

impl<'a> ExpandedTransaction<'a> {
	fn transaction(&self) -> &'a chain::Transaction {
		self.transaction
	}

	fn inputs(&self) -> ExpandedInputs {
		ExpandedInputs { transaction: self, position: 0 }
	}
}

pub struct ExpandedTransactions<'a> {
	block: &'a ExpandedBlock,
	position: usize,
}

impl<'a> Iterator for ExpandedTransactions<'a> {
	type Item = (&'a H256, ExpandedTransaction<'a>);

	fn next(&mut self) -> Option<(&'a H256, ExpandedTransaction<'a>)> {
		if self.position >= self.block.block.transaction_count() {
			None
		}
		else {
			let (hash, tx) = self.block.block.transaction_at(self.position);
			let result = Some((
				hash,
				ExpandedTransaction {
					transaction: tx,
					refs: &self.block.refs[self.position],
				},
			));
			self.position += 1;
			result
		}
	}
}

#[cfg(test)]
mod tests {

	use super::ExpandedBlock;
	use test_data;
	use devtools::RandomTempPath;
	use super::super::{Storage, BlockStapler};

	#[test]
	fn new() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let next_block = test_data::block_builder()
			.header().parent(genesis.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.output().build()
				.build()
			.build();

		let expanded_block = ExpandedBlock::new(next_block.into(), &store);

		assert_eq!(1, expanded_block.refs.len());
	}
}
