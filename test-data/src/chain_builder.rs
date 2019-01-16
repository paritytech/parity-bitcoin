use primitives::hash::H256;
use ser::Serializable;
use primitives::bytes::Bytes;
use chain::{Transaction, IndexedTransaction, TransactionInput, TransactionOutput, OutPoint};

#[derive(Debug, Default, Clone)]
pub struct ChainBuilder {
	pub transactions: Vec<Transaction>,
}

#[derive(Debug, Default, Clone)]
pub struct TransactionBuilder {
	pub transaction: Transaction,
}

impl ChainBuilder {
	pub fn new() -> ChainBuilder {
		ChainBuilder {
			transactions: Vec::new(),
		}
	}

	pub fn at(&self, transaction_index: usize) -> Transaction {
		self.transactions[transaction_index].clone()
	}

	pub fn hash(&self, transaction_index: usize) -> H256 {
		self.transactions[transaction_index].hash()
	}

	pub fn size(&self, transaction_index: usize) -> usize {
		self.transactions[transaction_index].serialized_size()
	}
}

impl Into<Transaction> for TransactionBuilder {
	fn into(self) -> Transaction {
		self.transaction
	}
}

impl Into<IndexedTransaction> for TransactionBuilder {
	fn into(self) -> IndexedTransaction {
		IndexedTransaction {
			hash: self.transaction.hash(),
			raw: self.transaction,
		}
	}
}

impl TransactionBuilder {
	pub fn with_version(version: i32) -> TransactionBuilder {
		let builder = TransactionBuilder::default();
		builder.set_version(version)
	}

	pub fn with_output(value: u64) -> TransactionBuilder {
		let builder = TransactionBuilder::default();
		builder.add_output(value)
	}

	pub fn with_default_input(output_index: u32) -> TransactionBuilder {
		let builder = TransactionBuilder::default();
		builder.add_input(&Transaction::default(), output_index)
	}

	pub fn with_input(transaction: &Transaction, output_index: u32) -> TransactionBuilder {
		let builder = TransactionBuilder::default();
		builder.add_input(transaction, output_index)
	}

	pub fn reset(self) -> TransactionBuilder {
		TransactionBuilder::default()
	}

	pub fn into_input(self, output_index: u32) -> TransactionBuilder {
		let builder = TransactionBuilder::default();
		builder.add_input(&self.transaction, output_index)
	}

	pub fn set_version(mut self, version: i32) -> TransactionBuilder {
		self.transaction.version = version;
		self
	}

	pub fn add_output(mut self, value: u64) -> TransactionBuilder {
		self.transaction.outputs.push(TransactionOutput {
			value: value,
			script_pubkey: Bytes::new_with_len(0),
		});
		self
	}

	pub fn set_output(mut self, value: u64) -> TransactionBuilder {
		self.transaction.outputs = vec![TransactionOutput {
			value: value,
			script_pubkey: Bytes::new_with_len(0),
		}];
		self
	}

	pub fn add_default_input(self, output_index: u32) -> TransactionBuilder {
		self.add_input(&Transaction::default(), output_index)
	}


	pub fn add_input(mut self, transaction: &Transaction, output_index: u32) -> TransactionBuilder {
		self.transaction.inputs.push(TransactionInput {
			previous_output: OutPoint {
				hash: transaction.hash(),
				index: output_index,
			},
			script_sig: Bytes::new_with_len(0),
			sequence: 0xffffffff,
			script_witness: vec![],
		});
		self
	}

	pub fn set_default_input(self, output_index: u32) -> TransactionBuilder {
		self.set_input(&Transaction::default(), output_index)
	}

	pub fn set_input(mut self, transaction: &Transaction, output_index: u32) -> TransactionBuilder {
		self.transaction.inputs = vec![TransactionInput {
			previous_output: OutPoint {
				hash: transaction.hash(),
				index: output_index,
			},
			script_sig: Bytes::new_with_len(0),
			sequence: 0xffffffff,
			script_witness: vec![],
		}];
		self
	}

	pub fn lock(mut self) -> Self {
		self.transaction.inputs[0].sequence = 0;
		self.transaction.lock_time = 500000;
		self
	}

	pub fn store(self, chain: &mut ChainBuilder) -> Self {
		chain.transactions.push(self.transaction.clone());
		self
	}

	pub fn hash(self) -> H256 {
		self.transaction.hash()
	}
}
