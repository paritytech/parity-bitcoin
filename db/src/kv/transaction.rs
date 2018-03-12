use bytes::Bytes;
use hash::H256;
use ser::{serialize, List, deserialize};
use chain::{Transaction as ChainTransaction, BlockHeader};
use storage::{TransactionMeta};

pub const COL_COUNT: u32 = 10;
pub const COL_META: u32 = 0;
pub const COL_BLOCK_HASHES: u32 = 1;
pub const COL_BLOCK_HEADERS: u32 = 2;
pub const COL_BLOCK_TRANSACTIONS: u32 = 3;
pub const COL_TRANSACTIONS: u32 = 4;
pub const COL_TRANSACTIONS_META: u32 = 5;
pub const COL_BLOCK_NUMBERS: u32 = 6;
pub const COL_CONFIGURATION: u32 = 7;

#[derive(Debug)]
pub enum Operation {
	Insert(KeyValue),
	Delete(Key),
}

#[derive(Debug)]
pub enum KeyValue {
	Meta(&'static str, Bytes),
	BlockHash(u32, H256),
	BlockHeader(H256, BlockHeader),
	BlockTransactions(H256, List<H256>),
	Transaction(H256, ChainTransaction),
	TransactionMeta(H256, TransactionMeta),
	BlockNumber(H256, u32),
	Configuration(&'static str, Bytes),
}

#[derive(Debug)]
pub enum Key {
	Meta(&'static str),
	BlockHash(u32),
	BlockHeader(H256),
	BlockTransactions(H256),
	Transaction(H256),
	TransactionMeta(H256),
	BlockNumber(H256),
	Configuration(&'static str),
}

#[derive(Debug, Clone)]
pub enum Value {
	Meta(Bytes),
	BlockHash(H256),
	BlockHeader(BlockHeader),
	BlockTransactions(List<H256>),
	Transaction(ChainTransaction),
	TransactionMeta(TransactionMeta),
	BlockNumber(u32),
	Configuration(Bytes),
}

impl Value {
	pub fn for_key(key: &Key, bytes: &[u8]) -> Result<Self, String> {
		match *key {
			Key::Meta(_) => deserialize(bytes).map(Value::Meta),
			Key::BlockHash(_) => deserialize(bytes).map(Value::BlockHash),
			Key::BlockHeader(_) => deserialize(bytes).map(Value::BlockHeader),
			Key::BlockTransactions(_) => deserialize(bytes).map(Value::BlockTransactions),
			Key::Transaction(_) => deserialize(bytes).map(Value::Transaction),
			Key::TransactionMeta(_) => deserialize(bytes).map(Value::TransactionMeta),
			Key::BlockNumber(_) => deserialize(bytes).map(Value::BlockNumber),
			Key::Configuration(_) => deserialize(bytes).map(Value::Configuration),
		}.map_err(|e| format!("{:?}", e))
	}

	pub fn as_meta(self) -> Option<Bytes> {
		match self {
			Value::Meta(bytes) => Some(bytes),
			_ => None,
		}
	}

	pub fn as_block_hash(self) -> Option<H256> {
		match self {
			Value::BlockHash(block_hash) => Some(block_hash),
			_ => None,
		}
	}

	pub fn as_block_header(self) -> Option<BlockHeader> {
		match self {
			Value::BlockHeader(block_header) => Some(block_header),
			_ => None,
		}
	}

	pub fn as_block_transactions(self) -> Option<List<H256>> {
		match self {
			Value::BlockTransactions(list) => Some(list),
			_ => None,
		}
	}

	pub fn as_transaction(self) -> Option<ChainTransaction> {
		match self {
			Value::Transaction(transaction) => Some(transaction),
			_ => None,
		}
	}

	pub fn as_transaction_meta(self) -> Option<TransactionMeta> {
		match self {
			Value::TransactionMeta(meta) => Some(meta),
			_ => None,
		}
	}

	pub fn as_block_number(self) -> Option<u32> {
		match self {
			Value::BlockNumber(number) => Some(number),
			_ => None,
		}
	}

	pub fn as_configuration(self) -> Option<Bytes> {
		match self {
			Value::Configuration(bytes) => Some(bytes),
			_ => None,
		}
	}
}

#[derive(Debug, Clone)]
pub enum KeyState<V> {
	Insert(V),
	Delete,
	Unknown,
}

impl<V> Default for KeyState<V> {
	fn default() -> Self {
		KeyState::Unknown
	}
}

impl<V> KeyState<V> {
	pub fn map<U, F>(self, f: F) -> KeyState<U> where F: FnOnce(V) -> U {
		match self {
			KeyState::Insert(value) => KeyState::Insert(f(value)),
			KeyState::Delete => KeyState::Delete,
			KeyState::Unknown => KeyState::Unknown,
		}
	}

	pub fn into_option(self) -> Option<V> {
		match self {
			KeyState::Insert(value) => Some(value),
			KeyState::Delete => None,
			KeyState::Unknown => None,
		}
	}

	pub fn into_operation<K, I, D>(self, key: K, insert: I, delete: D) -> Option<Operation>
	where I: FnOnce(K, V) -> KeyValue, D: FnOnce(K) -> Key {
		match self {
			KeyState::Insert(value) => Some(Operation::Insert(insert(key, value))),
			KeyState::Delete => Some(Operation::Delete(delete(key))),
			KeyState::Unknown => None,
		}
	}
}

#[derive(Debug)]
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
	pub fn new() -> Self {
		Transaction::default()
	}

	pub fn insert(&mut self, insert: KeyValue) {
		self.operations.push(Operation::Insert(insert));
	}

	pub fn delete(&mut self, delete: Key) {
		self.operations.push(Operation::Delete(delete));
	}
}

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

pub enum RawOperation {
	Insert(RawKeyValue),
	Delete(RawKey),
}

pub struct RawKeyValue {
	pub location: Location,
	pub key: Bytes,
	pub value: Bytes,
}

impl<'a> From<&'a KeyValue> for RawKeyValue {
	fn from(i: &'a KeyValue) -> Self {
		let (location, key, value) = match *i {
			KeyValue::Meta(ref key, ref value) => (COL_META, serialize(key), serialize(value)),
			KeyValue::BlockHash(ref key, ref value) => (COL_BLOCK_HASHES, serialize(key), serialize(value)),
			KeyValue::BlockHeader(ref key, ref value) => (COL_BLOCK_HEADERS, serialize(key), serialize(value)),
			KeyValue::BlockTransactions(ref key, ref value) => (COL_BLOCK_TRANSACTIONS, serialize(key), serialize(value)),
			KeyValue::Transaction(ref key, ref value) => (COL_TRANSACTIONS, serialize(key), serialize(value)),
			KeyValue::TransactionMeta(ref key, ref value) => (COL_TRANSACTIONS_META, serialize(key), serialize(value)),
			KeyValue::BlockNumber(ref key, ref value) => (COL_BLOCK_NUMBERS, serialize(key), serialize(value)),
			KeyValue::Configuration(ref key, ref value) => (COL_CONFIGURATION, serialize(key), serialize(value)),
		};

		RawKeyValue {
			location: location.into(),
			key: key,
			value: value,
		}
	}
}

pub struct RawKey {
	pub location: Location,
	pub key: Bytes,
}

impl RawKey {
	pub fn new<B>(location: Location, key: B) -> Self where B: Into<Bytes> {
		RawKey {
			location: location,
			key: key.into(),
		}
	}
}

impl<'a> From<&'a Key> for RawKey {
	fn from(d: &'a Key) -> Self {
		let (location, key) = match *d {
			Key::Meta(ref key) => (COL_META, serialize(key)),
			Key::BlockHash(ref key) => (COL_BLOCK_HASHES, serialize(key)),
			Key::BlockHeader(ref key) => (COL_BLOCK_HEADERS, serialize(key)),
			Key::BlockTransactions(ref key) => (COL_BLOCK_TRANSACTIONS, serialize(key)),
			Key::Transaction(ref key) => (COL_TRANSACTIONS, serialize(key)),
			Key::TransactionMeta(ref key) => (COL_TRANSACTIONS_META, serialize(key)),
			Key::BlockNumber(ref key) => (COL_BLOCK_NUMBERS, serialize(key)),
			Key::Configuration(ref key) => (COL_CONFIGURATION, serialize(key)),
		};

		RawKey {
			location: location.into(),
			key: key,
		}
	}
}

impl<'a> From<&'a Operation> for RawOperation {
	fn from(o: &'a Operation) -> Self {
		match *o {
			Operation::Insert(ref insert) => RawOperation::Insert(insert.into()),
			Operation::Delete(ref delete) => RawOperation::Delete(delete.into()),
		}
	}
}

pub struct RawTransaction {
	pub operations: Vec<RawOperation>,
}

impl<'a> From<&'a Transaction> for RawTransaction {
	fn from(tx: &'a Transaction) -> Self {
		RawTransaction {
			operations: tx.operations.iter().map(Into::into).collect()
		}
	}
}

impl Default for RawTransaction {
	fn default() -> Self {
		RawTransaction {
			operations: Vec::with_capacity(32),
		}
	}
}

impl RawTransaction {
	pub fn new() -> RawTransaction {
		RawTransaction::default()
	}

	pub fn insert_raw(&mut self, location: Location, key: &[u8], value: &[u8]) {
		let operation = RawOperation::Insert(RawKeyValue {
			location: location,
			key: key.into(),
			value: value.into(),
		});
		self.operations.push(operation);
	}

	pub fn delete_raw(&mut self, location: Location, key: &[u8]) {
		let operation = RawOperation::Delete(RawKey {
			location: location,
			key: key.into(),
		});
		self.operations.push(operation);
	}
}
