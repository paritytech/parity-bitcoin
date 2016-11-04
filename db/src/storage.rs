//! Bitcoin storage

use std::{self, fs};
use std::path::Path;
use kvdb::{DBTransaction, Database, DatabaseConfig};
use byteorder::{LittleEndian, ByteOrder};
use primitives::hash::H256;
use primitives::bytes::Bytes;
use super::{BlockRef, BestBlock};
use serialization;
use chain::{self, RepresentH256};
use parking_lot::RwLock;
use transaction_meta::TransactionMeta;
use std::collections::HashMap;

const COL_COUNT: u32 = 10;
const COL_META: u32 = 0;
const COL_BLOCK_HASHES: u32 = 1;
const COL_BLOCK_HEADERS: u32 = 2;
const COL_BLOCK_TRANSACTIONS: u32 = 3;
const COL_TRANSACTIONS: u32 = 4;
const COL_TRANSACTIONS_META: u32 = 5;
const COL_BLOCK_NUMBERS: u32 = 6;
const _COL_RESERVED3: u32 = 7;
const _COL_RESERVED4: u32 = 8;
const _COL_RESERVED5: u32 = 9;
const _COL_RESERVED6: u32 = 10;

const DB_VERSION: u32 = 1;

/// Blockchain storage interface
pub trait Store : Send + Sync {
	/// get best block
	fn best_block(&self) -> Option<BestBlock>;

	/// resolves number by block hash
	fn block_number(&self, hash: &H256) -> Option<u32>;

	/// resolves hash by block number
	fn block_hash(&self, number: u32) -> Option<H256>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	/// resolves list of block transactions by block reference (number/hash)
	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256>;

	/// resolves transaction body bytes by transaction hash
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes>;

	/// resolves serialized transaction info by transaction hash
	fn transaction(&self, hash: &H256) -> Option<chain::Transaction>;

	/// returns all transactions in the block by block reference (number/hash)
	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction>;

	/// resolves deserialized block body by block reference (number/hash)
	fn block(&self, block_ref: BlockRef) -> Option<chain::Block>;

	/// returns true if store contains given block
	fn contains_block(&self, block_ref: BlockRef) -> bool {
		self.block_header_bytes(block_ref).is_some()
	}

	/// insert block in the storage
	fn insert_block(&self, block: &chain::Block) -> Result<(), Error>;

	/// get transaction metadata
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta>;
}

/// Blockchain storage with rocksdb database
pub struct Storage {
	database: Database,
	best_block: RwLock<Option<BestBlock>>,
}

#[derive(Debug)]
pub enum MetaError {
	UnsupportedVersion,
}

#[derive(Debug)]
/// Database error
pub enum Error {
	/// Rocksdb error
	DB(String),
	/// Io error
	Io(std::io::Error),
	/// Invalid meta info
	Meta(MetaError),
	/// Unknown hash
	Unknown(H256),
	/// Not the block from the main chain
	NotMain(H256),
}

impl From<String> for Error {
	fn from(err: String) -> Error {
		Error::DB(err)
	}
}

impl From<std::io::Error> for Error {
	fn from(err: std::io::Error) -> Error {
		Error::Io(err)
	}
}

fn u32_key(num: u32) -> [u8; 4] {
	let mut result = [0u8; 4];
	LittleEndian::write_u32(&mut result, num);
	result
}

const KEY_VERSION: &'static[u8] = b"version";
const KEY_BEST_BLOCK_NUMBER: &'static[u8] = b"best_block_number";
const KEY_BEST_BLOCK_HASH: &'static[u8] = b"best_block_hash";

struct UpdateContext {
	pub meta: HashMap<H256, TransactionMeta>,
	pub db_transaction: DBTransaction,
}

impl UpdateContext {
	pub fn new(db: &Database) -> Self {
		UpdateContext {
			meta: HashMap::new(),
			db_transaction: db.transaction(),
		}
	}

	pub fn apply(mut self, db: &Database) -> Result<(), Error> {
		// actually saving meta
		for (hash, meta) in self.meta.drain() {
			self.db_transaction.put(Some(COL_TRANSACTIONS_META), &*hash, &meta.to_bytes());
		}

		try!(db.write(self.db_transaction));
		Ok(())
	}
}

impl Storage {

	/// new storage at the selected path
	/// if no directory exists, it will be created
	pub fn new<P: AsRef<Path>>(path: P) -> Result<Storage, Error> {
		try!(fs::create_dir_all(path.as_ref()));
		let cfg = DatabaseConfig::with_columns(Some(COL_COUNT));
		let db = try!(Database::open(&cfg, &*path.as_ref().to_string_lossy()));

		let storage = Storage {
			database: db,
			best_block: RwLock::default(),
		};

		match storage.read_meta_u32(KEY_VERSION) {
			Some(ver) => {
				if ver != DB_VERSION {
					return Err(Error::Meta(MetaError::UnsupportedVersion))
				}
			},
			_ => {
				let mut meta_transaction = storage.database.transaction();
				meta_transaction.write_u32(Some(COL_META), KEY_VERSION, DB_VERSION);
				try!(storage.database.write(meta_transaction));
			},
		};

		let best_number = storage.read_meta_u32(KEY_BEST_BLOCK_NUMBER);
		let best_hash = storage.get(COL_META, KEY_BEST_BLOCK_HASH).map(|val| H256::from(&**val));

		// both values should be stored
		assert!(best_number.is_some() == best_hash.is_some());
		if best_number.is_some() {
			*storage.best_block.write() = Some(
				BestBlock {
					number: best_number.expect("is_some() is checked above for block number"),
					hash: best_hash.expect("is_some() is checked above for block hash"),
				}
			);
		}

		Ok(storage)
	}

	fn read_meta(&self, key: &[u8]) -> Option<Bytes> {
		self.get(COL_META, key)
	}

	fn read_meta_u32(&self, key: &[u8]) -> Option<u32> {
		self.read_meta(key).map(|val| LittleEndian::read_u32(&val))
	}

	/// is invoked on database non-fatal query errors
	fn db_error(&self, msg: String) {
		println!("Low-level database error: {}", &msg);
	}

	/// get the value of the key in the database
	/// if the key is not present, reports non-fatal error and returns nothing
	fn get(&self, col: u32, key: &[u8]) -> Option<Bytes> {
		let res = self.database.get(Some(col), key);
		match res {
			Err(msg) => {
				self.db_error(msg);
				None
			},
			Ok(val) => val.map(|v| v.into()),
		}
	}

	/// resolves hash for the block reference (which can be referenced by number or
	/// by hash)
	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}

	/// loads block transaction list by the provided block hash
	fn block_transaction_hashes_by_hash(&self, h: &H256) -> Vec<H256> {
		self.get(COL_BLOCK_TRANSACTIONS, &**h)
			.unwrap_or(Vec::new().into())
			.chunks(H256::size())
			.map(H256::from)
			.collect()
	}

	fn block_transactions_by_hash(&self, h: &H256) -> Vec<chain::Transaction> {
		self.block_transaction_hashes_by_hash(h)
			.into_iter()
			.filter_map(|tx_hash| {
				self.transaction_bytes(&tx_hash).and_then(|tx_bytes| {
					match serialization::deserialize::<_, chain::Transaction>(tx_bytes.as_ref()) {
						Ok(tx) => Some(tx),
						Err(e) => {
							self.db_error(format!("Error deserializing transaction, possible db corruption ({:?})", e));
							None
						}
					}
				})
			})
			.collect()
	}

	/// update transactions metadata in the specified database transaction
	fn update_transactions_meta(&self, db_transaction: &mut DBTransaction, number: u32, accepted_txs: &[chain::Transaction]) {
		let mut meta_buf = HashMap::<H256, TransactionMeta>::new();

		// inserting new meta for coinbase transaction
		for accepted_tx in accepted_txs.iter() {
			// adding unspent transaction meta
			meta_buf.insert(accepted_tx.hash(), TransactionMeta::new(number, accepted_tx.outputs.len()));
		}

		// another iteration skipping coinbase transaction
		for accepted_tx in accepted_txs.iter().skip(1) {
			for input in accepted_tx.inputs.iter() {
				if !match meta_buf.get_mut(&input.previous_output.hash) {
					Some(ref mut meta) => {
						meta.note_used(input.previous_output.index as usize);
						true
					},
					None => false,
				} {
					let mut meta =
						self.transaction_meta(&input.previous_output.hash)
							.unwrap_or_else(|| panic!(
								"No transaction metadata for {}! Corrupted DB? Reindex?",
								&input.previous_output.hash
							));

					meta.note_used(input.previous_output.index as usize);

					meta_buf.insert(
						input.previous_output.hash.clone(),
						meta);
				}
			}
		}

		// actually saving meta
		for (hash, meta) in meta_buf.drain() {
			db_transaction.put(Some(COL_TRANSACTIONS_META), &*hash, &meta.to_bytes());
		}
	}

	/// block decanonization
	///   all transaction outputs used are marked as not used
	///   all transaction meta is removed
	///   DOES NOT update best block
	fn decanonize_block(&self, context: &mut UpdateContext, hash: &H256) -> Result<(), Error> {
		let block_height = try!(self.block_number(hash).ok_or(Error::NotMain(hash.clone())));

		let tx_hashes = self.block_transaction_hashes_by_hash(hash);
		for (tx_hash_num, tx_hash) in tx_hashes.iter().enumerate() {
			let tx = self.transaction(tx_hash)
				.expect("Transaction in the saved block should exist as a separate entity indefinitely");

			// remove meta
			context.db_transaction.delete(Some(COL_TRANSACTIONS_META), &**tx_hash);

			// denote outputs used
			if tx_hash_num == 0 { continue; } // coinbase transaction does not have inputs
			for input in tx.inputs.iter() {
				if !match context.meta.get_mut(&input.previous_output.hash) {
					Some(ref mut meta) => {
						meta.denote_used(input.previous_output.index as usize);
						true
					},
					None => false,
				} {
					let mut meta =
						self.transaction_meta(&input.previous_output.hash)
							.unwrap_or_else(|| panic!(
								"No transaction metadata for {}! Corrupted DB? Reindex?",
								&input.previous_output.hash
							));

					meta.denote_used(input.previous_output.index as usize);

					context.meta.insert(
						input.previous_output.hash.clone(),
						meta);
				}
			}
		}

		Ok(())
	}
}

impl Store for Storage {
	fn best_block(&self) -> Option<BestBlock> {
		self.best_block.read().clone()
	}

	fn block_number(&self, hash: &H256) -> Option<u32> {
		self.get(COL_BLOCK_NUMBERS, &**hash)
			.map(|val| LittleEndian::read_u32(&val))
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		self.get(COL_BLOCK_HASHES, &u32_key(number))
			.map(|val| H256::from(&**val))
	}

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.resolve_hash(block_ref).and_then(|h| self.get(COL_BLOCK_HEADERS, &*h))
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transaction_hashes_by_hash(&h))
			.unwrap_or(Vec::new())
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transactions_by_hash(&h))
			.unwrap_or(Vec::new())
	}

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.get(COL_TRANSACTIONS, &**hash)
	}

	fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
		self.resolve_hash(block_ref).and_then(|block_hash|
			self.get(COL_BLOCK_HEADERS, &*block_hash)
				.and_then(|header_bytes| {
					let transactions = self.block_transactions_by_hash(&block_hash);;
					let maybe_header = match serialization::deserialize::<_, chain::BlockHeader>(header_bytes.as_ref()) {
						Ok(header) => Some(header),
						Err(e) => {
							self.db_error(format!("Error deserializing header, possible db corruption ({:?})", e));
							None
						}
					};
					maybe_header.map(|header| chain::Block::new(header, transactions))
			})
		)
	}

	fn insert_block(&self, block: &chain::Block) -> Result<(), Error> {
		let mut best_block = self.best_block.write();

		let block_hash = block.hash();

		let new_best_hash = match best_block.as_ref().map(|bb| &bb.hash) {
			Some(best_hash) if &block.header().previous_header_hash != best_hash => best_hash.clone(),
			_ => block_hash.clone(),
		};

		let new_best_number = match best_block.as_ref().map(|b| b.number) {
			Some(best_number) => {
				if block.hash() == new_best_hash { best_number + 1 }
				else { best_number }
			},
			None => 0,
		};

		let mut transaction = self.database.transaction();

		let tx_space = block.transactions().len() * 32;
		let mut tx_refs = Vec::with_capacity(tx_space);
		for tx in block.transactions() {
			let tx_hash = tx.hash();
			tx_refs.extend(&*tx_hash);
			transaction.put(
				Some(COL_TRANSACTIONS),
				&*tx_hash,
				&serialization::serialize(tx),
			);
		}
		transaction.put(Some(COL_BLOCK_TRANSACTIONS), &*block_hash, &tx_refs);

		transaction.put(
			Some(COL_BLOCK_HEADERS),
			&*block_hash,
			&serialization::serialize(block.header())
		);

		if best_block.as_ref().map(|b| b.number) != Some(new_best_number) {
			self.update_transactions_meta(&mut transaction, new_best_number, block.transactions());
			transaction.write_u32(Some(COL_META), KEY_BEST_BLOCK_NUMBER, new_best_number);

			// updating main chain height reference
			transaction.put(Some(COL_BLOCK_HASHES), &u32_key(new_best_number), std::ops::Deref::deref(&block_hash));
			transaction.write_u32(Some(COL_BLOCK_NUMBERS), std::ops::Deref::deref(&block_hash), new_best_number);
		}

		transaction.put(Some(COL_META), KEY_BEST_BLOCK_HASH, std::ops::Deref::deref(&new_best_hash));

		try!(self.database.write(transaction));

		*best_block = Some(BestBlock { hash: new_best_hash, number: new_best_number });

		Ok(())
	}

	fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
		self.transaction_bytes(hash).and_then(|tx_bytes| {
			serialization::deserialize(tx_bytes.as_ref()).map_err(
				|e| self.db_error(format!("Error deserializing transaction, possible db corruption ({:?})", e))
			).ok()
		})
	}

	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		self.get(COL_TRANSACTIONS_META, &**hash).map(|val|
			TransactionMeta::from_bytes(&val).unwrap_or_else(|e| panic!("Invalid transaction metadata: db corrupted? ({:?})", e))
		)
	}
}

#[cfg(test)]
mod tests {

	use super::{Storage, Store, UpdateContext};
	use devtools::RandomTempPath;
	use chain::{Block, RepresentH256};
	use super::super::BlockRef;
	use test_data;

	#[test]
	fn open_store() {
		let path = RandomTempPath::create_dir();
		assert!(Storage::new(path.as_path()).is_ok());
	}

	#[test]
	fn insert_block() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block_h1();
		store.insert_block(&block).unwrap();

		let loaded_block = store.block(BlockRef::Hash(block.hash())).unwrap();
		assert_eq!(loaded_block.hash(), block.hash());
	}


	#[test]
	fn insert_genesis() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::genesis();
		store.insert_block(&block).unwrap();

		assert_eq!(store.best_block().expect("genesis block inserted").number, 0);
		assert_eq!(store.block_hash(0), Some(block.hash()));
	}

	#[test]
	fn best_block_update() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis: Block = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let block: Block = test_data::block_h1();
		store.insert_block(&block).unwrap();

		assert_eq!(store.best_block().expect("genesis block inserted").number, 1);
	}

	#[test]
	fn best_hash_update_fork() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block_h1();
		store.insert_block(&block).unwrap();

		let another_block: Block = test_data::block_h9();
		store.insert_block(&another_block).unwrap();

		// did not update because `another_block` is not child of `block`
		assert_eq!(store.best_block().expect("blocks inserted above").hash, block.hash());
		// number should not be update also
		assert_eq!(store.best_block().expect("blocks inserted above").number, 0);
	}

	#[test]
	fn load_transaction() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block_h9();
		store.insert_block(&block).unwrap();

		let block: Block = test_data::block_h170();
		let tx1 = block.transactions()[0].hash();
		store.insert_block(&block).unwrap();

		let loaded_transaction = store.transaction(&tx1).unwrap();
		assert_eq!(loaded_transaction.hash(), block.transactions()[0].hash());
	}

	#[test]
	fn stores_block_number() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let block: Block = test_data::block_h9();
		store.insert_block(&block).unwrap();

		let number = store.block_number(&block.hash()).unwrap();
		assert_eq!(0, number);
	}

	#[test]
	fn transaction_meta_update() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let genesis_coinbase = genesis.transactions()[0].hash();

		let genesis_meta = store.transaction_meta(&genesis_coinbase).unwrap();
		assert!(!genesis_meta.is_spent(0));

		let forged_block = test_data::block_builder()
			.header().parent(genesis.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.output().build()
				.build()
			.build();

		store.insert_block(&forged_block).unwrap();

		let genesis_meta = store.transaction_meta(&genesis_coinbase).unwrap();
		assert!(genesis_meta.is_spent(0));

		assert_eq!(store.best_block().expect("genesis block inserted").number, 1);
	}

	#[test]
	fn transaction_meta_same_block() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let block = test_data::block_builder()
			.header().parent(genesis.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(30).build()
				.output().value(20).build()
				.build()
			.derived_transaction(1, 0)
				.output().value(30).build()
				.build()
			.build();

		store.insert_block(&block).unwrap();

		let meta = store.transaction_meta(&block.transactions()[1].hash()).unwrap();
		assert!(meta.is_spent(0), "Transaction #1 first output in the new block should be recorded as spent");
		assert!(!meta.is_spent(1), "Transaction #1 second output in the new block should be recorded as unspent");
	}

	#[test]
	fn transaction_meta_complex() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let block1 = test_data::block_builder()
			.header().parent(genesis.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(10).build()
				.output().value(15).build()
				.output().value(10).build()
				.output().value(1).build()
				.output().value(4).build()
				.output().value(10).build()
				.build()
			.build();

		store.insert_block(&block1).unwrap();

		let tx_big = block1.transactions()[1].hash();
		let block2 = test_data::block_builder()
			.header().parent(block1.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(tx_big.clone()).index(0).build()
				.input().hash(tx_big.clone()).index(2).build()
				.input().hash(tx_big.clone()).index(5).build()
				.output().value(30).build()
				.build()
			.build();

		store.insert_block(&block2).unwrap();

		let meta = store.transaction_meta(&tx_big).unwrap();
		assert!(meta.is_spent(0), "Transaction #1 output #0 in the new block should be recorded as spent");
		assert!(meta.is_spent(2), "Transaction #1 output #2 in the new block should be recorded as spent");
		assert!(meta.is_spent(5), "Transaction #1 output #5 in the new block should be recorded as spent");

		assert!(!meta.is_spent(1), "Transaction #1 output #1 in the new block should be recorded as unspent");
		assert!(!meta.is_spent(3), "Transaction #1 second #3 in the new block should be recorded as unspent");
	}

	#[test]
	fn fork_smoky() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let (_main_hash1, main_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(1)
					.build()
				.build()
			.build();

		store.insert_block(&main_block1).expect("main block 1 should insert with no problems");

		let (side_hash1, side_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(2)
					.build()
				.build()
			.build();

		store.insert_block(&side_block1).expect("side block 1 should insert with no problems");

		let (side_hash2, side_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(side_hash1)
					.nonce(3)
					.build()
				.build()
			.build();

		store.insert_block(&side_block2).expect("side block 2 should insert with no problems");

		// store should reorganize to side hash 2, because it represents the longer chain
		assert_eq!(store.best_block().unwrap().hash, side_hash2);
	}


	// test simulates when main chain and side chain are competing all along, each adding
	// block one by one
	#[test]
	fn fork_competing() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let (main_hash1, main_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(1)
					.build()
				.build()
			.build();

		store.insert_block(&main_block1).expect("main block 1 should insert with no problems");

		let (side_hash1, side_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(2)
					.build()
				.build()
			.build();

		store.insert_block(&side_block1).expect("side block 1 should insert with no problems");

		let (main_hash2, main_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(main_hash1)
					.nonce(3)
					.build()
				.build()
			.build();

		store.insert_block(&main_block2).expect("main block 2 should insert with no problems");

		let (_side_hash2, side_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(side_hash1)
					.nonce(4)
					.build()
				.build()
			.build();

		store.insert_block(&side_block2).expect("side block 2 should insert with no problems");

		// store should not reorganize to side hash 2, because it competing chains are of the equal length
		assert_eq!(store.best_block().unwrap().hash, main_hash2);
	}

	#[test]
	fn decanonize() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let block = test_data::block_builder()
			.header().parent(genesis.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.build()
			.build();

		store.insert_block(&block).expect("inserting first block in the decanonize test should not fail");

		let genesis_meta = store.transaction_meta(&genesis_coinbase)
			.expect("Transaction meta for the genesis coinbase transaction should exist");
		assert!(genesis_meta.is_spent(0), "Genesis coinbase should be recorded as spent because block#1 transaction spends it");

		let mut update_context = UpdateContext::new(&store.database);
		store.decanonize_block(&mut update_context, &block.hash())
			.expect("Decanonizing block #1 which was just inserted should not fail");
		update_context.apply(&store.database);

		let genesis_meta = store.transaction_meta(&genesis_coinbase)
			.expect("Transaction meta for the genesis coinbase transaction should exist");
		assert!(!genesis_meta.is_spent(0), "Genesis coinbase should be recorded as unspent because we retracted block #1");
	}
}
