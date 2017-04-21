use kv::{Location, Transaction, Value, DatabaseKey};

pub trait KeyValueDatabase: Send + Sync {
	fn write(&self, tx: Transaction) -> Result<(), String>;

	fn get<Key, Value>(&self, key: &Key) -> Result<Option<Value>, String> where Key: DatabaseKey<Value>;
}
