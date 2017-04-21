use std::collections::HashMap;
use std::sync::Arc;
use std::mem::replace;
use parking_lot::RwLock;
use hash::H256;
use bytes::Bytes;
use ser::List;
use chain::{Transaction as ChainTransaction, BlockHeader};
use kv::{Transaction, Key, KeyState, Location, Operation, Value, KeyValueDatabase, DatabaseKey, Insert, Delete};
use {TransactionMeta};

#[derive(Default, Debug)]
struct InnerDatabase {
	meta: HashMap<&'static str, KeyState<Bytes>>,
	block_hash: HashMap<u32, KeyState<H256>>,
	block_header: HashMap<H256, KeyState<BlockHeader>>,
	block_transactions: HashMap<H256, KeyState<List<H256>>>,
	transaction: HashMap<H256, KeyState<ChainTransaction>>,
	transaction_meta: HashMap<H256, KeyState<TransactionMeta>>,
	block_number: HashMap<H256, KeyState<u32>>,
}

#[derive(Default, Debug)]
pub struct MemoryDatabase {
	db: RwLock<InnerDatabase>,
}

impl MemoryDatabase {
	pub fn drain_transaction(&self) -> Transaction {
		let mut db = self.db.write();
		let meta = replace(&mut db.meta, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::Meta(key, value)),
				KeyState::Delete => Operation::Delete(Delete::Meta(key)),
			});
		let block_hash = replace(&mut db.block_hash, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::BlockHash(key, value)),
				KeyState::Delete => Operation::Delete(Delete::BlockHash(key)),
			});
		let block_header = replace(&mut db.block_header, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::BlockHeader(key, value)),
				KeyState::Delete => Operation::Delete(Delete::BlockHeader(key)),
			});
		let block_transactions = replace(&mut db.block_transactions, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::BlockTransactions(key, value)),
				KeyState::Delete => Operation::Delete(Delete::BlockTransactions(key)),
			});
		let transaction = replace(&mut db.transaction, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::Transaction(key, value)),
				KeyState::Delete => Operation::Delete(Delete::Transaction(key)),
			});
		let transaction_meta = replace(&mut db.transaction_meta, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::TransactionMeta(key, value)),
				KeyState::Delete => Operation::Delete(Delete::TransactionMeta(key)),
			});
		let block_number = replace(&mut db.block_number, HashMap::default()).into_iter()
			.map(|(key, state)| match state {
				KeyState::Insert(value) => Operation::Insert(Insert::BlockNumber(key, value)),
				KeyState::Delete => Operation::Delete(Delete::BlockNumber(key)),
			});

		Transaction {
			operations: meta
				.chain(block_hash)
				.chain(block_header)
				.chain(block_transactions)
				.chain(transaction)
				.chain(transaction_meta)
				.chain(block_number)
				.collect()
		}
	}
}

impl KeyValueDatabase for MemoryDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		let mut db = self.db.write();
		for op in tx.operations.into_iter() {
			match op {
				Operation::Insert(insert) => match insert {
					Insert::Meta(key, value) => { db.meta.insert(key, KeyState::Insert(value)); },
					Insert::BlockHash(key, value) => { db.block_hash.insert(key, KeyState::Insert(value)); },
					Insert::BlockHeader(key, value) => { db.block_header.insert(key, KeyState::Insert(value)); },
					Insert::BlockTransactions(key, value) => { db.block_transactions.insert(key, KeyState::Insert(value)); },
					Insert::Transaction(key, value) => { db.transaction.insert(key, KeyState::Insert(value)); },
					Insert::TransactionMeta(key, value) => { db.transaction_meta.insert(key, KeyState::Insert(value)); },
					Insert::BlockNumber(key, value) => { db.block_number.insert(key, KeyState::Insert(value)); },
				},
				Operation::Delete(delete) => match delete {
					Delete::Meta(key) => { db.meta.insert(key, KeyState::Delete); }
					Delete::BlockHash(key) => { db.block_hash.insert(key, KeyState::Delete); }
					Delete::BlockHeader(key) => { db.block_header.insert(key, KeyState::Delete); }
					Delete::BlockTransactions(key) => { db.block_transactions.insert(key, KeyState::Delete); }
					Delete::Transaction(key) => { db.transaction.insert(key, KeyState::Delete); }
					Delete::TransactionMeta(key) => { db.transaction_meta.insert(key, KeyState::Delete); }
					Delete::BlockNumber(key) => { db.block_number.insert(key, KeyState::Delete); }
				}
			}
		}
		Ok(())
	}

	fn get<Key, Value>(&self, key: &Key) -> Result<Option<Value>, String> where Key: DatabaseKey<Value> {
		unimplemented!();
		//use kv::transaction::{COL_META};

		//let db = self.db.read();

		//let result = match Key::location() {
			//Location::Column(COL_META) => db.meta.get(key).and_then(|state| state.clone().into_option()),
			//_ => { unimplemented!(); }
		//};
		//Ok(result)
	//fn get(&self, location: Location, key: &Key) -> Result<Option<Value>, String> {
		//match self.db.read().get(&location).and_then(|db| db.get(key)) {
			//Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
			//Some(&KeyState::Delete) | None => Ok(None),
		//}
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

	fn get<Key, Value>(&self, key: &Key) -> Result<Option<Value>, String> where Key: DatabaseKey<Value> {
		unimplemented!();
	//fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String> {
		//self.db.get(location, key)
	}
}
