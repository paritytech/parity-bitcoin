use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use kv::{Transaction, Key, KeyState, Location, Operation, Value, KeyValueDatabase};

#[derive(Debug)]
pub struct MemoryDatabase {
	db: RwLock<HashMap<Location, HashMap<Key, KeyState>>>,
}

impl Default for MemoryDatabase {
	fn default() -> Self {
		MemoryDatabase {
			db: RwLock::default(),
		}
	}
}

impl MemoryDatabase {
	pub fn drain_transaction(&self) -> Transaction {
		let mut db = self.db.write();
		let operations = db.drain()
			.flat_map(|(location, action)| {
				action.into_iter().map(|(key, state)| match state {
					KeyState::Insert(value) => Operation::Insert {
						location: location,
						key: key,
						value: value,
					},
					KeyState::Delete => Operation::Delete {
						location: location,
						key: key,
					}
				})
				.collect::<Vec<_>>()
			})
			.collect();
		Transaction {
			operations: operations,
		}
	}

	pub fn is_known(&self, location: Location, key: &[u8]) -> bool {
		self.db.read().get(&location).and_then(|db| db.get(key)).is_some()
	}
}

impl KeyValueDatabase for MemoryDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		let mut db = self.db.write();
		for op in tx.operations.into_iter() {
			match op {
				Operation::Insert { location, key, value } => {
					let db = db.entry(location).or_insert_with(HashMap::default);
					db.insert(key, KeyState::Insert(value));
				},
				Operation::Delete { location, key } => {
					let db = db.entry(location).or_insert_with(HashMap::default);
					db.insert(key, KeyState::Delete);
				},
			}
		}
		Ok(())
	}

	fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String> {
		match self.db.read().get(&location).and_then(|db| db.get(key)) {
			Some(&KeyState::Insert(ref value)) => Ok(Some(value.clone())),
			Some(&KeyState::Delete) | None => Ok(None),
		}
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

	fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String> {
		self.db.get(location, key)
	}
}
