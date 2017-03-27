use kv::{Location, Transaction, Value};

pub trait KeyValueDatabase: Send + Sync {
	fn write(&self, tx: Transaction) -> Result<(), String>;

	fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String>;
}
