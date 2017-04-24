use lru_cache::LruCache;
use parking_lot::Mutex;
use hash::H256;
use chain::BlockHeader;
use kv::{KeyValueDatabase, KeyState, Operation, KeyValue, Key, Value, Transaction};

pub struct CacheDatabase<T> where T: KeyValueDatabase {
	db: T,
	header: Mutex<LruCache<H256, KeyState<BlockHeader>>>,
}

impl<T> CacheDatabase<T> where T: KeyValueDatabase {
	pub fn new(db: T) -> Self {
		CacheDatabase {
			db: db,
			// 144 (blocks per day) * 14 (days) + 100 (arbitrary number)
			header: Mutex::new(LruCache::new(2116)),
		}
	}
}

impl<T> KeyValueDatabase for CacheDatabase<T> where T: KeyValueDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String> {
		for op in &tx.operations {
			match *op {
				Operation::Insert(KeyValue::BlockHeader(ref hash, ref header)) => {
					self.header.lock().insert(hash.clone(), KeyState::Insert(header.clone()));
				},
				Operation::Delete(Key::BlockHeader(ref hash)) => {
					self.header.lock().insert(hash.clone(), KeyState::Delete);
				},
				_ => (),
			}
		}
		self.db.write(tx)
	}

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String> {
		if let Key::BlockHeader(ref hash) = *key {
			let mut header = self.header.lock();
			if let Some(state) = header.get_mut(hash) {
				return Ok(state.clone().map(Value::BlockHeader))
			}
		}
		self.db.get(key)
	}
}
