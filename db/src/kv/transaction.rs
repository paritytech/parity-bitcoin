use elastic_array::{ElasticArray32, ElasticArray128};
use bytes::Bytes;
use hash::H256;
use ser::{Serializable, serialize, List};
use chain::{Transaction as ChainTransaction, BlockHeader};
use {TransactionMeta};

pub const COL_META: u32 = 0;
pub const COL_BLOCK_HASHES: u32 = 1;
pub const COL_BLOCK_HEADERS: u32 = 2;
pub const COL_BLOCK_TRANSACTIONS: u32 = 3;
pub const COL_TRANSACTIONS: u32 = 4;
pub const COL_TRANSACTIONS_META: u32 = 5;
pub const COL_BLOCK_NUMBERS: u32 = 6;

pub type Key = Bytes;
pub type Value = Bytes;

#[derive(Debug)]
pub enum Operation {
	Insert(Insert),
	Delete(Delete),
}

#[derive(Debug)]
pub enum Insert {
	Meta(&'static str, Bytes),
	BlockHash(u32, H256),
	BlockHeader(H256, BlockHeader),
	BlockTransactions(H256, List<H256>),
	Transaction(H256, ChainTransaction),
	TransactionMeta(H256, TransactionMeta),
	BlockNumber(H256, u32),
}

#[derive(Debug)]
pub enum Delete {
	Meta(&'static str),
	BlockHash(u32),
	BlockHeader(H256),
	BlockTransactions(H256),
	Transaction(H256),
	TransactionMeta(H256),
	BlockNumber(H256),
}

#[derive(Debug, Clone)]
pub enum KeyState<Value> {
	Insert(Value),
	Delete,
}

impl<Value> KeyState<Value> {
	pub fn into_option(self) -> Option<Value> {
		match self {
			KeyState::Insert(value) => Some(value),
			KeyState::Delete => None,
		}
	}
}

pub trait DatabaseKey<Value> {
	fn location() -> Location;
}

impl DatabaseKey<Bytes> for &'static str {
	fn location() -> Location {
		COL_META.into()
	}
}

impl DatabaseKey<H256> for u32 {
	fn location() -> Location {
		COL_BLOCK_HASHES.into()
	}
}

impl DatabaseKey<BlockHeader> for H256 {
	fn location() -> Location {
		COL_BLOCK_HEADERS.into()
	}
}

impl DatabaseKey<List<H256>> for H256 {
	fn location() -> Location {
		COL_BLOCK_TRANSACTIONS.into()
	}
}

impl DatabaseKey<ChainTransaction> for H256 {
	fn location() -> Location {
		COL_TRANSACTIONS.into()
	}
}

impl DatabaseKey<TransactionMeta> for H256 {
	fn location() -> Location {
		COL_TRANSACTIONS_META.into()
	}
}

impl DatabaseKey<u32> for H256 {
	fn location() -> Location {
		COL_BLOCK_NUMBERS.into()
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

	pub fn insert(&mut self, insert: Insert) {
		self.operations.push(Operation::Insert(insert));
	}

	pub fn delete(&mut self, delete: Delete) {
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

fn raw_insert<Key, Value>(key: &Key, value: &Value) -> RawOperation where Key: DatabaseKey<Value> + Serializable, Value: Serializable {
	RawOperation::Insert {
		location: Key::location(),
		key: serialize(key),
		value: serialize(value),
	}
}

fn raw_delete<Key, Value>(key: &Key) -> RawOperation where Key: DatabaseKey<Value> + Serializable {
	RawOperation::Delete {
		location: Key::location(),
		key: serialize(key),
	}
}

impl From<Operation> for RawOperation {
	fn from(o: Operation) -> Self {
		match o {
			Operation::Insert(insert) => match insert {
				Insert::Meta(key, value) => raw_insert(&key, &value),
				Insert::BlockHash(key, value) => raw_insert(&key, &value),
				Insert::BlockHeader(key, value) => raw_insert(&key, &value),
				Insert::BlockTransactions(key, value) => raw_insert(&key, &value),
				Insert::Transaction(key, value) => raw_insert(&key, &value),
				Insert::TransactionMeta(key, value) => raw_insert(&key, &value),
				Insert::BlockNumber(key, value) => raw_insert(&key, &value),
			},
			Operation::Delete(delete) => match delete {
				Delete::Meta(key) => raw_delete::<_, Bytes>(&key),
				Delete::BlockHash(key) => raw_delete::<_, H256>(&key),
				Delete::BlockHeader(key) => raw_delete::<_, BlockHeader>(&key),
				Delete::BlockTransactions(key) => raw_delete::<_, List<H256>>(&key),
				Delete::Transaction(key) => raw_delete::<_, ChainTransaction>(&key),
				Delete::TransactionMeta(key) => raw_delete::<_, TransactionMeta>(&key),
				Delete::BlockNumber(key) => raw_delete::<_, u32>(&key),
			},
		}
	}
}

pub struct RawTransaction {
	pub operations: Vec<RawOperation>,
}

impl From<Transaction> for RawTransaction {
	fn from(tx: Transaction) -> Self {
		RawTransaction {
			operations: tx.operations.into_iter().map(Into::into).collect()
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
}
