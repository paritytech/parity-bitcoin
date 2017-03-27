use kv::{Location, Transaction, Value};

pub trait KeyValueDatabase {
	fn write(&self, tx: Transaction) -> Result<(), String>;

	fn get(&self, location: Location, key: &[u8]) -> Result<Option<Value>, String>;
}
