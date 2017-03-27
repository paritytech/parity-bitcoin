use elastic_array::{ElasticArray32, ElasticArray128};

pub type Key = ElasticArray32<u8>;
pub type Value = ElasticArray128<u8>;

#[derive(Debug, PartialEq, Eq, Hash)]
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

pub struct Transaction {
	pub operations: Vec<Operation>,
}

impl Default for Transaction {
	fn default() -> Self {
		Transaction {
			operations: Vec::with_capacity(256),
		}
	}
}

impl Transaction {
	pub fn new() -> Transaction {
		Transaction::default()
	}

	pub fn insert(&mut self, location: Location, key: &[u8], value: &[u8]) {
		let operation = Operation::Insert {
			location: location,
			key: Key::from_slice(key),
			value: Value::from_slice(value),
		};
		self.operations.push(operation);
	}

	pub fn delete(&mut self, location: Location, key: &[u8]) {
		let operation = Operation::Delete {
			location: location,
			key: Key::from_slice(key),
		};
		self.operations.push(operation);
	}
}
