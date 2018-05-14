use bytes::Bytes;
use hash::H256;
use ser::{serialize, List, deserialize};
use chain::{BlockHeader, OutPoint};
use storage::{TransactionMeta, TransactionPrunableData, TransactionOutputMeta};

/// Total number of columns in database.
pub const COL_COUNT: u32 = 10;
/// Database metadata information.
pub const COL_META: u32 = 0;
/// Mapping of { block height => block hash }.
/// Only contains entries for canonized blocks.
pub const COL_BLOCK_HASHES: u32 = 1;
/// Mapping of { block hash => block number }.
/// Only contains entries for canonized blocks.
pub const COL_BLOCK_NUMBERS: u32 = 2;
/// Mapping of { block hash => block header }.
/// Can be pruned for ancient blocks.
pub const COL_BLOCK_HEADERS: u32 = 3;
/// Mapping of { block hash => block transactions hashes }.
/// Can be pruned for ancient blocks.
pub const COL_BLOCK_TRANSACTIONS: u32 = 4;
/// Mapping of { transaction hash => prunable transaction data }.
/// Contains all tx fields that can be pruned when tx block becames ancient:
/// - version: is only used for serialization/deserialization;
/// - lock_time: is only used when tx is verified
/// - inputs: are used only when tx is verified
/// - total number of transaction outputs
pub const COL_TRANSACTION_PRUNABLE: u32 = 5;
/// Mapping of { transaction hash => non-prunable transaction meta data }.
/// Only contains entries for transactions from canonized blocks.
/// Contains:
/// - height of block for coinbase transactions (to check coinbase maturity)
/// - total number of transaction outputs
/// - total number of spent transaction outputs
pub const COL_TRANSACTION_META: u32 = 6;
/// Mapping of { { transaction hash, index } => transaction output }.
/// Only contains entries for:
/// - spent outputs
/// - outputs of non-canonized transactions
/// Can be pruned for spent outputs, which have been spent in transactions from ancient blocks.
pub const COL_TRANSACTION_OUTPUT: u32 = 7;

#[derive(Debug)]
pub enum Operation {
	Insert(KeyValue),
	Delete(Key),
}

#[derive(Debug)]
pub enum KeyValue {
	Meta(&'static str, Bytes),
	BlockHash(u32, H256),
	BlockNumber(H256, u32),
	BlockHeader(H256, BlockHeader),
	BlockTransactions(H256, List<H256>),
	TransactionPrunable(H256, TransactionPrunableData),
	TransactionMeta(H256, TransactionMeta),
	TransactionOutput(OutPoint, TransactionOutputMeta),
}

#[derive(Debug)]
pub enum Key {
	Meta(&'static str),
	BlockHash(u32),
	BlockNumber(H256),
	BlockHeader(H256),
	BlockTransactions(H256),
	TransactionPrunable(H256),
	TransactionMeta(H256),
	TransactionOutput(OutPoint),
}

#[derive(Debug, Clone)]
pub enum Value {
	Meta(Bytes),
	BlockHash(H256),
	BlockNumber(u32),
	BlockHeader(BlockHeader),
	BlockTransactions(List<H256>),
	TransactionPrunable(TransactionPrunableData),
	TransactionMeta(TransactionMeta),
	TransactionOutput(TransactionOutputMeta),
}

impl Value {
	pub fn for_key(key: &Key, bytes: &[u8]) -> Result<Self, String> {
		match *key {
			Key::Meta(_) => deserialize(bytes).map(Value::Meta),
			Key::BlockHash(_) => deserialize(bytes).map(Value::BlockHash),
			Key::BlockNumber(_) => deserialize(bytes).map(Value::BlockNumber),
			Key::BlockHeader(_) => deserialize(bytes).map(Value::BlockHeader),
			Key::BlockTransactions(_) => deserialize(bytes).map(Value::BlockTransactions),
			Key::TransactionPrunable(_) => deserialize(bytes).map(Value::TransactionPrunable),
			Key::TransactionMeta(_) => deserialize(bytes).map(Value::TransactionMeta),
			Key::TransactionOutput(_) => deserialize(bytes).map(Value::TransactionOutput),
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

	pub fn as_block_number(self) -> Option<u32> {
		match self {
			Value::BlockNumber(number) => Some(number),
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

	pub fn as_transaction_prunable(self) -> Option<TransactionPrunableData> {
		match self {
			Value::TransactionPrunable(transaction_prunable) => Some(transaction_prunable),
			_ => None,
		}
	}

	pub fn as_transaction_meta(self) -> Option<TransactionMeta> {
		match self {
			Value::TransactionMeta(meta) => Some(meta),
			_ => None,
		}
	}

	pub fn as_transaction_output(self) -> Option<TransactionOutputMeta> {
		match self {
			Value::TransactionOutput(output) => Some(output),
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

#[derive(Debug)]
pub enum RawOperation {
	Insert(RawKeyValue),
	Delete(RawKey),
}

#[derive(Debug)]
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
			KeyValue::BlockNumber(ref key, ref value) => (COL_BLOCK_NUMBERS, serialize(key), serialize(value)),
			KeyValue::BlockHeader(ref key, ref value) => (COL_BLOCK_HEADERS, serialize(key), serialize(value)),
			KeyValue::BlockTransactions(ref key, ref value) => (COL_BLOCK_TRANSACTIONS, serialize(key), serialize(value)),
			KeyValue::TransactionPrunable(ref key, ref value) => (COL_TRANSACTION_PRUNABLE, serialize(key), serialize(value)),
			KeyValue::TransactionMeta(ref key, ref value) => (COL_TRANSACTION_META, serialize(key), serialize(value)),
			KeyValue::TransactionOutput(ref key, ref value) => (COL_TRANSACTION_OUTPUT, serialize(key), serialize(value)),
		};

		RawKeyValue {
			location: location.into(),
			key: key,
			value: value,
		}
	}
}

#[derive(Debug)]
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
			Key::BlockNumber(ref key) => (COL_BLOCK_NUMBERS, serialize(key)),
			Key::BlockHeader(ref key) => (COL_BLOCK_HEADERS, serialize(key)),
			Key::BlockTransactions(ref key) => (COL_BLOCK_TRANSACTIONS, serialize(key)),
			Key::TransactionPrunable(ref key) => (COL_TRANSACTION_PRUNABLE, serialize(key)),
			Key::TransactionMeta(ref key) => (COL_TRANSACTION_META, serialize(key)),
			Key::TransactionOutput(ref key) => (COL_TRANSACTION_OUTPUT, serialize(key)),
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
