use kv::{Transaction, KeyState, Key, Value};

pub trait KeyValueDatabase: Send + Sync {
	fn write(&self, tx: Transaction) -> Result<(), String>;

	fn get(&self, key: &Key) -> Result<KeyState<Value>, String>;
}
