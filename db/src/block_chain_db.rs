use std::collections::HashMap;
use std::fs;
use std::path::Path;
use parking_lot::RwLock;
use hash::H256;
use bytes::Bytes;
use chain::{
	IndexedBlock, IndexedBlockHeader, IndexedTransaction, BlockHeader, Block, Transaction,
	OutPoint, TransactionOutput
};
use ser::{
	deserialize, serialize, serialize_list, Serializable, Deserializable,
	DeserializableList
};
use kv::{
	KeyValueDatabase, OverlayDatabase, Transaction as DBTransaction, Location, Value, DiskDatabase,
	DatabaseConfig, MemoryDatabase
};
use best_block::BestBlock;
use {
	BlockRef, Error, BlockHeaderProvider, BlockProvider, BlockOrigin, TransactionMeta, IndexedBlockProvider,
	TransactionMetaProvider, TransactionProvider, PreviousTransactionOutputProvider, BlockChain, Store,
	SideChainOrigin, ForkChain, TransactionOutputObserver, Forkable, CanonStore
};

const COL_COUNT: u32 = 10;
const COL_META: u32 = 0;
const COL_BLOCK_HASHES: u32 = 1;
const COL_BLOCK_HEADERS: u32 = 2;
const COL_BLOCK_TRANSACTIONS: u32 = 3;
const COL_TRANSACTIONS: u32 = 4;
const COL_TRANSACTIONS_META: u32 = 5;
const COL_BLOCK_NUMBERS: u32 = 6;

const KEY_VERSION: &'static[u8] = b"version";
const KEY_BEST_BLOCK_NUMBER: &'static[u8] = b"best_block_number";
const KEY_BEST_BLOCK_HASH: &'static[u8] = b"best_block_hash";

const DB_VERSION: u32 = 1;
const MAX_FORK_ROUTE_PRESET: usize = 2048;

pub struct BlockChainDatabase<T> where T: KeyValueDatabase {
	best_block: RwLock<BestBlock>,
	db: T,
}

pub struct ForkChainDatabase<'a, T> where T: 'a + KeyValueDatabase {
	blockchain: BlockChainDatabase<OverlayDatabase<'a, T>>,
}

impl<'a, T> ForkChain for ForkChainDatabase<'a, T> where T: KeyValueDatabase {
	fn store(&self) -> &Store {
		&self.blockchain
	}

	fn flush(&self) -> Result<(), Error> {
		self.blockchain.db.flush().map_err(Error::DatabaseError)
	}
}

impl BlockChainDatabase<DiskDatabase> {
	pub fn open_at_path<P>(path: P, total_cache: usize) -> Result<Self, Error> where P: AsRef<Path> {
		fs::create_dir_all(path.as_ref()).map_err(|err| Error::DatabaseError(err.to_string()))?;
		let mut cfg = DatabaseConfig::with_columns(Some(COL_COUNT));

		cfg.set_cache(Some(COL_TRANSACTIONS), total_cache / 4);
		cfg.set_cache(Some(COL_TRANSACTIONS_META), total_cache / 4);
		cfg.set_cache(Some(COL_BLOCK_HEADERS), total_cache / 4);

		cfg.set_cache(Some(COL_BLOCK_HASHES), total_cache / 12);
		cfg.set_cache(Some(COL_BLOCK_TRANSACTIONS), total_cache / 12);
		cfg.set_cache(Some(COL_BLOCK_NUMBERS), total_cache / 12);

		cfg.bloom_filters.insert(Some(COL_TRANSACTIONS_META), 32);

		match DiskDatabase::open(cfg, path) {
			Ok(db) => Ok(Self::open(db)),
			Err(err) => Err(Error::DatabaseError(err))
		}
	}
}

impl BlockChainDatabase<MemoryDatabase> {
	pub fn init_test_chain(blocks: Vec<IndexedBlock>) -> Self {
		let store = BlockChainDatabase::open(MemoryDatabase::default());

		for block in &blocks {
			store.insert(block).unwrap();
			store.canonize(block.hash()).unwrap();
		}
		store
	}
}

impl<T> BlockChainDatabase<T> where T: KeyValueDatabase {
	fn read_best_block(db: &T) -> Option<BestBlock> {
		let best_number = db.get(COL_META.into(), KEY_BEST_BLOCK_NUMBER);
		let best_hash = db.get(COL_META.into(), KEY_BEST_BLOCK_HASH);

		match (best_number, best_hash) {
			(Ok(None), Ok(None)) => None,
			(Ok(Some(number)), Ok(Some(hash))) => Some(BestBlock {
				number: deserialize(&*number).expect("Inconsistent DB. Invalid best block number."),
				hash: deserialize(&*hash).expect("Inconsistent DB. Invalid best block hash."),
			}),
			_ => panic!("Inconsistent DB"),
		}
	}

	pub fn open(db: T) -> Self {
		let best_block = Self::read_best_block(&db).unwrap_or_default();
		BlockChainDatabase {
			best_block: RwLock::new(best_block),
			db: db,
		}
	}

	pub fn best_block(&self) -> BestBlock {
		self.best_block.read().clone()
	}

	pub fn fork(&self, side_chain: SideChainOrigin) -> Result<ForkChainDatabase<T>, Error> {
		let overlay = BlockChainDatabase::open(OverlayDatabase::new(&self.db));

		for hash in side_chain.decanonized_route.into_iter().rev() {
			let decanonized_hash = overlay.decanonize()?;
			assert_eq!(hash, decanonized_hash);
		}

		for block_hash in &side_chain.canonized_route {
			overlay.canonize(block_hash)?;
		}

		let fork = ForkChainDatabase {
			blockchain: overlay,
		};

		Ok(fork)
	}

	pub fn switch_to_fork(&self, fork: ForkChainDatabase<T>) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		*best_block = fork.blockchain.best_block.read().clone();
		fork.blockchain.db.flush().map_err(Error::DatabaseError)
	}

	pub fn block_origin(&self, header: &IndexedBlockHeader) -> Result<BlockOrigin, Error> {
		let best_block = self.best_block.read();
		assert_eq!(Some(best_block.hash.clone()), self.block_hash(best_block.number));
		if self.contains_block(header.hash.clone().into()) {
			// it does not matter if it's canon chain or side chain block
			return Ok(BlockOrigin::KnownBlock)
		}

		if best_block.hash == header.raw.previous_header_hash {
			return Ok(BlockOrigin::CanonChain {
				block_number: best_block.number + 1
			})
		}

		if !self.contains_block(header.raw.previous_header_hash.clone().into()) {
			return Err(Error::UnknownParent)
		}

		let mut sidechain_route = Vec::new();
		let mut next_hash = header.raw.previous_header_hash.clone();

		for fork_len in 0..MAX_FORK_ROUTE_PRESET {
			match self.block_number(&next_hash) {
				Some(number) => {
					let block_number = number + fork_len as u32 + 1;
					let origin = SideChainOrigin {
						ancestor: number,
						canonized_route: sidechain_route.into_iter().rev().collect(),
						decanonized_route: (number + 1..best_block.number + 1).into_iter()
							//.map(|decanonized_bn| self.block_hash(decanonized_bn + 1).expect("to find block hashes of canon chain blocks; qed"))
							.filter_map(|decanonized_bn| self.block_hash(decanonized_bn))
							.collect(),
						block_number: block_number,
					};
					if block_number > best_block.number {
						return Ok(BlockOrigin::SideChainBecomingCanonChain(origin))
					} else {
						return Ok(BlockOrigin::SideChain(origin))
					}
				},
				None => {
					sidechain_route.push(next_hash.clone());
					next_hash = self.block_header(next_hash.into())
						.expect("not to find orphaned side chain in database; qed")
						.previous_header_hash;
				}
			}
		}

		Err(Error::AncientFork)
	}

	pub fn insert(&self, block: &IndexedBlock) -> Result<(), Error> {
		if self.contains_block(block.hash().clone().into()) {
			return Ok(())
		}

		let parent_hash = &block.header.raw.previous_header_hash;
		if !self.contains_block(parent_hash.clone().into()) && !parent_hash.is_zero() {
			return Err(Error::UnknownParent);
		}

		let mut update = DBTransaction::new();
		update.insert(COL_BLOCK_HEADERS.into(), block.hash(), &block.header.raw);
		for tx in &block.transactions {
			update.insert(COL_TRANSACTIONS.into(), &tx.hash, &tx.raw);
		}

		let tx_hashes = serialize_list::<H256, &H256>(&block.transactions.iter().map(|tx| &tx.hash).collect::<Vec<_>>());
		update.insert_raw(COL_BLOCK_TRANSACTIONS.into(), &**block.hash(), &tx_hashes);
		self.db.write(update).map_err(Error::DatabaseError)
	}

	/// Marks block as a new best block.
	/// Block must be already inserted into db, and it's parent must be current best block.
	/// Updates meta data.
	pub fn canonize(&self, hash: &H256) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		let block = match self.indexed_block(hash.clone().into()) {
			Some(block) => block,
			None => return Err(Error::CannotCanonize),
		};

		if best_block.hash != block.header.raw.previous_header_hash {
			return Err(Error::CannotCanonize);
		}

		let new_best_block = BestBlock {
			hash: hash.clone(),
			number: if block.header.raw.previous_header_hash.is_zero() {
				assert_eq!(best_block.number, 0);
				0
			} else {
				best_block.number + 1
			}
		};

		trace!(target: "db", "canonize {:?}", new_best_block);

		let mut update = DBTransaction::new();
		update.insert(COL_BLOCK_HASHES.into(), &new_best_block.number, &new_best_block.hash);
		update.insert(COL_BLOCK_NUMBERS.into(), &new_best_block.hash, &new_best_block.number);
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_HASH, &serialize(&new_best_block.hash));
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_NUMBER, &serialize(&new_best_block.number));

		let mut modified_meta: HashMap<H256, TransactionMeta> = HashMap::new();
		if let Some(tx) = block.transactions.first() {
			let meta = TransactionMeta::new_coinbase(new_best_block.number, tx.raw.outputs.len());
			modified_meta.insert(tx.hash.clone(), meta);
		}

		for tx in block.transactions.iter().skip(1) {
			modified_meta.insert(tx.hash.clone(), TransactionMeta::new(new_best_block.number, tx.raw.outputs.len()));

			for input in &tx.raw.inputs {
				use std::collections::hash_map::Entry;

				match modified_meta.entry(input.previous_output.hash.clone()) {
					Entry::Occupied(mut entry) => {
						let meta = entry.get_mut();
						meta.denote_used(input.previous_output.index as usize);
					},
					Entry::Vacant(entry) => {
						let mut meta = self.transaction_meta(&input.previous_output.hash)
							.ok_or(Error::CannotCanonize)?;
						meta.denote_used(input.previous_output.index as usize);
						entry.insert(meta);
					}
				}
			}
		}

		for (hash, meta) in modified_meta.iter() {
			update.insert(COL_TRANSACTIONS_META.into(), hash, meta);
		}

		self.db.write(update).map_err(Error::DatabaseError)?;
		*best_block = new_best_block;
		Ok(())
	}

	pub fn decanonize(&self) -> Result<H256, Error> {
		let mut best_block = self.best_block.write();
		let block = match self.indexed_block(best_block.hash.clone().into()) {
			Some(block) => block,
			None => return Err(Error::CannotCanonize),
		};
		let block_number = best_block.number;
		let block_hash = best_block.hash.clone();

		let new_best_block = BestBlock {
			hash: block.header.raw.previous_header_hash.clone(),
			number: if best_block.number > 0 {
				best_block.number - 1
			} else {
				assert!(block.header.raw.previous_header_hash.is_zero());
				0
			}
		};

		trace!(target: "db", "decanonize, new best: {:?}", new_best_block);

		let mut update = DBTransaction::new();
		update.delete(COL_BLOCK_HASHES.into(), &block_number);
		update.delete(COL_BLOCK_NUMBERS.into(), &block_hash);
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_HASH, &serialize(&new_best_block.hash));
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_NUMBER, &serialize(&new_best_block.number));

		let mut modified_meta: HashMap<H256, TransactionMeta> = HashMap::new();
		for tx in block.transactions.iter().skip(1) {
			for input in &tx.raw.inputs {
				use std::collections::hash_map::Entry;

				match modified_meta.entry(input.previous_output.hash.clone()) {
					Entry::Occupied(mut entry) => {
						let meta = entry.get_mut();
						meta.denote_unused(input.previous_output.index as usize);
					},
					Entry::Vacant(entry) => {
						let mut meta = self.transaction_meta(&input.previous_output.hash)
							.ok_or(Error::CannotCanonize)?;
						meta.denote_unused(input.previous_output.index as usize);
						entry.insert(meta);
					}
				}
			}
		}

		for (hash, meta) in modified_meta.iter() {
			update.insert(COL_TRANSACTIONS_META.into(), hash, meta);
		}

		for tx in &block.transactions {
			update.delete(COL_TRANSACTIONS_META.into(), &tx.hash);
		}

		self.db.write(update).map_err(Error::DatabaseError)?;
		*best_block = new_best_block;
		Ok(block_hash)
	}

	fn get_raw<K>(&self, location: Location, key: &K) -> Option<Value> where K: Serializable {
		self.db.get(location, &serialize(key)).expect("db query to be fine")
	}

	fn get<K, V>(&self, location: Location, key: &K) -> Option<V> where K: Serializable, V: Deserializable {
		self.get_raw(location, key).map(|val| deserialize(&*val).expect("db value to be fine"))
	}

	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}
}

impl<T> BlockHeaderProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.resolve_hash(block_ref)
			.and_then(|hash| self.get_raw(COL_BLOCK_HEADERS.into(), &hash))
			.map(|raw| (&*raw).into())
	}

	fn block_header(&self, block_ref: BlockRef) -> Option<BlockHeader> {
		self.resolve_hash(block_ref)
			.and_then(|hash| self.get(COL_BLOCK_HEADERS.into(), &hash))
	}
}

impl<T> BlockProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn block_number(&self, hash: &H256) -> Option<u32> {
		self.get(COL_BLOCK_NUMBERS.into(), hash)
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		self.get(COL_BLOCK_HASHES.into(), &number)
	}

	fn block(&self, block_ref: BlockRef) -> Option<Block> {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| {
				self.block_header(block_hash.clone().into())
					.map(|header| {
						let transactions = self.block_transactions(block_hash.into());
						Block::new(header, transactions)
					})
			})
	}

	fn contains_block(&self, block_ref: BlockRef) -> bool {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| self.get_raw(COL_BLOCK_HEADERS.into(), &block_hash))
			.is_some()
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| self.get(COL_BLOCK_TRANSACTIONS.into(), &block_hash))
			.map(|hashes: DeserializableList<H256>| hashes.into())
			.unwrap_or_default()
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<Transaction> {
		self.block_transaction_hashes(block_ref)
			.iter()
			.filter_map(|hash| self.get(COL_TRANSACTIONS.into(), hash))
			.collect()
	}
}

impl<T> IndexedBlockProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn indexed_block_header(&self, block_ref: BlockRef) -> Option<IndexedBlockHeader> {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| {
				self.get(COL_BLOCK_HEADERS.into(), &block_hash)
					.map(|header| IndexedBlockHeader::new(block_hash, header))
			})
	}

	fn indexed_block(&self, block_ref: BlockRef) -> Option<IndexedBlock> {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| {
				self.indexed_block_header(block_hash.clone().into())
					.map(|header| {
						let transactions = self.indexed_block_transactions(block_hash.into());
						IndexedBlock::new(header, transactions)
					})
			})
	}

	fn indexed_block_transactions(&self, block_ref: BlockRef) -> Vec<IndexedTransaction> {
		self.block_transaction_hashes(block_ref)
			.into_iter()
			.filter_map(|hash| {
				self.get(COL_TRANSACTIONS.into(), &hash)
					.map(|tx| IndexedTransaction::new(hash, tx))
			})
			.collect()
	}
}

impl<T> TransactionMetaProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		self.get(COL_TRANSACTIONS_META.into(), hash)
	}
}

impl<T> TransactionProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.get_raw(COL_TRANSACTIONS.into(), hash)
			.map(|raw| (&*raw).into())
	}

	fn transaction(&self, hash: &H256) -> Option<Transaction> {
		self.get(COL_TRANSACTIONS.into(), hash)
	}
}

impl<T> PreviousTransactionOutputProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn previous_transaction_output(&self, prevout: &OutPoint, _transaction_index: usize) -> Option<TransactionOutput> {
		// return previous transaction outputs only for canon chain transactions
		self.transaction_meta(&prevout.hash)
			.and_then(|_| self.transaction(&prevout.hash))
			.and_then(|tx| tx.outputs.into_iter().nth(prevout.index as usize))
	}
}

impl<T> TransactionOutputObserver for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn is_spent(&self, prevout: &OutPoint) -> Option<bool> {
		self.transaction_meta(&prevout.hash)
			.and_then(|meta| meta.is_spent(prevout.index as usize))
	}
}

impl<T> BlockChain for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn insert(&self, block: &IndexedBlock) -> Result<(), Error> {
		BlockChainDatabase::insert(self, block)
	}

	fn canonize(&self, block_hash: &H256) -> Result<(), Error> {
		BlockChainDatabase::canonize(self, block_hash)
	}

	fn decanonize(&self) -> Result<H256, Error> {
		BlockChainDatabase::decanonize(self)
	}

	fn block_origin(&self, header: &IndexedBlockHeader) -> Result<BlockOrigin, Error> {
		BlockChainDatabase::block_origin(self, header)
	}
}

impl<T> Forkable for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn fork<'a>(&'a self, side_chain: SideChainOrigin) -> Result<Box<ForkChain + 'a>, Error> {
		BlockChainDatabase::fork(self, side_chain)
			.map(|fork_chain| {
				let boxed: Box<ForkChain> = Box::new(fork_chain);
				boxed
			})
	}

	fn switch_to_fork<'a>(&self, fork: Box<ForkChain + 'a>) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		*best_block = fork.store().best_block();
		fork.flush()
	}
}

impl<T> CanonStore for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn as_store(&self) -> &Store {
		&*self
	}
}

impl<T> Store for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn best_block(&self) -> BestBlock {
		BlockChainDatabase::best_block(self)
	}

	/// get best header
	fn best_header(&self) -> BlockHeader {
		self.block_header(self.best_block().hash.into()).expect("best block header should be in db; qed")
	}

	/// get blockchain difficulty
	fn difficulty(&self) -> f64 {
		self.best_header().bits.to_f64()
	}
}
