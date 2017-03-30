use elastic_array::{ElasticArray32, ElasticArray128};
use ser::{Serializable, serialize};

pub type Key = ElasticArray32<u8>;
pub type Value = ElasticArray128<u8>;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum Location {
	DB,
	Column(u32),
}

impl From<u32> for Location {
	fn from(column: u32) -> Location {
		Location::Column(column)
	}
}

pub enum Operation {
	Insert {
		location: Location,
		key: Key,
		value: Value,
	},
	Delete {
		location: Location,
		key: Key,
	}
}

#[derive(Debug)]
pub enum KeyState {
	Insert(Value),
	Delete,
}

pub struct Transaction {
	pub operations: Vec<Operation>,
}

impl Default for Transaction {
	fn default() -> Self {
		Transaction {
			operations: Vec::with_capacity(32),
		}
	}
}

impl Transaction {
	pub fn new() -> Transaction {
		Transaction::default()
	}

	pub fn insert_raw(&mut self, location: Location, key: &[u8], value: &[u8]) {
		let operation = Operation::Insert {
			location: location,
			key: Key::from_slice(key),
			value: Value::from_slice(value),
		};
		self.operations.push(operation);
	}

	pub fn insert<K, V>(&mut self, location: Location, key: &K, value: &V) where K: Serializable, V: Serializable {
		self.insert_raw(location, &serialize(key), &serialize(value))
	}

	pub fn delete_raw(&mut self, location: Location, key: &[u8]) {
		let operation = Operation::Delete {
			location: location,
			key: Key::from_slice(key),
		};
		self.operations.push(operation);
	}

	pub fn delete<K>(&mut self, location: Location, key: &K) where K: Serializable {
		self.delete_raw(location, &serialize(key))
	}
}
