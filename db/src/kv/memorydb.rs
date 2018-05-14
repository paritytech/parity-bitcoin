use std::collections::HashMap;
use std::sync::Arc;
use std::mem::replace;
use parking_lot::RwLock;
use hash::H256;
use bytes::Bytes;
use ser::List;
use chain::{BlockHeader, OutPoint};
use kv::{Transaction, Key, KeyState, Operation, Value, KeyValueDatabase, KeyValue};
use storage::{TransactionMeta, TransactionPrunableData, TransactionOutputMeta};

#[derive(Default, Debug)]
pub struct MemoryDatabase {
	db: RwLock<MemoryDatabaseCore>,
}

#[derive(Default, Debug)]
pub struct MemoryDatabaseCore {
	pub meta: HashMap<&'static str, KeyState<Bytes>>,
	pub block_hash: HashMap<u32, KeyState<H256>>,
	pub block_number: HashMap<H256, KeyState<u32>>,
	pub block_header: HashMap<H256, KeyState<BlockHeader>>,
	pub block_transactions: HashMap<H256, KeyState<List<H256>>>,
	pub transaction_prunable: HashMap<H256, KeyState<TransactionPrunableData>>,
	pub transaction_meta: HashMap<H256, KeyState<TransactionMeta>>,
	pub transaction_output: HashMap<(H256, u32), KeyState<TransactionOutputMeta>>,
}

impl MemoryDatabase {
	pub fn drain_transaction(&self) -> Transaction {
		Transaction {
			operations: self.db.write().drain_operations(),
		}
	}
}

impl KeyValueDatabase for MemoryDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		let mut db = self.db.write();
		for op in tx.operations.into_iter() {
			db.write(op);
		}
		Ok(())
	}

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		self.db.read().get(key)
	}
}

impl MemoryDatabaseCore {
	pub fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		let result = match *key {
			Key::Meta(ref key) => self.meta.get(key).cloned().unwrap_or_default().map(Value::Meta),
			Key::BlockHash(ref key) => self.block_hash.get(key).cloned().unwrap_or_default().map(Value::BlockHash),
			Key::BlockHeader(ref key) => self.block_header.get(key).cloned().unwrap_or_default().map(Value::BlockHeader),
			Key::BlockNumber(ref key) => self.block_number.get(key).cloned().unwrap_or_default().map(Value::BlockNumber),
			Key::BlockTransactions(ref key) => self.block_transactions.get(key).cloned().unwrap_or_default().map(Value::BlockTransactions),
			Key::TransactionPrunable(ref key) => self.transaction_prunable.get(key).cloned().unwrap_or_default().map(Value::TransactionPrunable),
			Key::TransactionMeta(ref key) => self.transaction_meta.get(key).cloned().unwrap_or_default().map(Value::TransactionMeta),
			Key::TransactionOutput(ref key) => self.transaction_output.get(&(key.hash.clone(), key.index)).cloned().unwrap_or_default().map(Value::TransactionOutput),
		};

		Ok(result)
	}

	pub fn write(&mut self, op: Operation) {
		match op {
			Operation::Insert(insert) => match insert {
				KeyValue::Meta(key, value) => { self.meta.insert(key, KeyState::Insert(value)); },
				KeyValue::BlockHash(key, value) => { self.block_hash.insert(key, KeyState::Insert(value)); },
				KeyValue::BlockNumber(key, value) => { self.block_number.insert(key, KeyState::Insert(value)); },
				KeyValue::BlockHeader(key, value) => { self.block_header.insert(key, KeyState::Insert(value)); },
				KeyValue::BlockTransactions(key, value) => { self.block_transactions.insert(key, KeyState::Insert(value)); },
				KeyValue::TransactionPrunable(key, value) => { self.transaction_prunable.insert(key, KeyState::Insert(value)); },
				KeyValue::TransactionMeta(key, value) => { self.transaction_meta.insert(key, KeyState::Insert(value)); },
				KeyValue::TransactionOutput(key, value) => { self.transaction_output.insert((key.hash, key.index), KeyState::Insert(value)); },
			},
			Operation::Delete(delete) => match delete {
				Key::Meta(key) => { self.meta.insert(key, KeyState::Delete); }
				Key::BlockHash(key) => { self.block_hash.insert(key, KeyState::Delete); }
				Key::BlockNumber(key) => { self.block_number.insert(key, KeyState::Delete); }
				Key::BlockHeader(key) => { self.block_header.insert(key, KeyState::Delete); }
				Key::BlockTransactions(key) => { self.block_transactions.insert(key, KeyState::Delete); }
				Key::TransactionPrunable(key) => { self.transaction_prunable.insert(key, KeyState::Delete); }
				Key::TransactionMeta(key) => { self.transaction_meta.insert(key, KeyState::Delete); }
				Key::TransactionOutput(key) => { self.transaction_output.insert((key.hash, key.index), KeyState::Delete); },
			}
		}
	}

	pub fn drain_operations(&mut self) -> Vec<Operation> {
		let meta = replace(&mut self.meta, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::Meta, Key::Meta));

		let block_hash = replace(&mut self.block_hash, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::BlockHash, Key::BlockHash));

		let block_number = replace(&mut self.block_number, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::BlockNumber, Key::BlockNumber));

		let block_header = replace(&mut self.block_header, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::BlockHeader, Key::BlockHeader));

		let block_transactions = replace(&mut self.block_transactions, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::BlockTransactions, Key::BlockTransactions));

		let transaction_prunable = replace(&mut self.transaction_prunable, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::TransactionPrunable, Key::TransactionPrunable));

		let transaction_meta = replace(&mut self.transaction_meta, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(key, KeyValue::TransactionMeta, Key::TransactionMeta));

		let transaction_output = replace(&mut self.transaction_output, HashMap::default()).into_iter()
			.flat_map(|(key, state)| state.into_operation(OutPoint { hash: key.0, index: key.1 }, KeyValue::TransactionOutput, Key::TransactionOutput));

		meta
			.chain(block_hash)
			.chain(block_header)
			.chain(block_number)
			.chain(block_transactions)
			.chain(transaction_prunable)
			.chain(transaction_meta)
			.chain(transaction_output)
			.collect()
	}
}

#[derive(Debug)]
pub struct SharedMemoryDatabase {
	db: Arc<MemoryDatabase>,
}

impl Default for SharedMemoryDatabase {
	fn default() -> Self {
		SharedMemoryDatabase {
			db: Arc::default(),
		}
	}
}

impl Clone for SharedMemoryDatabase {
	fn clone(&self) -> Self {
		SharedMemoryDatabase {
			db: self.db.clone(),
		}
	}
}

impl KeyValueDatabase for SharedMemoryDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		self.db.write(tx)
	}

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		self.db.get(key)
	}
}
