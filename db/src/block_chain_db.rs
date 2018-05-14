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
	deserialize, serialize, List
};
use kv::{
	KeyValueDatabase, OverlayDatabase, Transaction as DBTransaction, Value, DiskDatabase,
	DatabaseConfig, MemoryDatabase, AutoFlushingOverlayDatabase, KeyValue, Key, KeyState, CacheDatabase,
	MemoryDatabaseCore, Operation, overlay_get, overlay_write,
};
use kv::{
	COL_COUNT, COL_BLOCK_HASHES, COL_BLOCK_NUMBERS, COL_BLOCK_HEADERS,
	COL_BLOCK_TRANSACTIONS, COL_TRANSACTION_PRUNABLE, COL_TRANSACTION_META,
	COL_TRANSACTION_OUTPUT,
};
use storage::{
	BlockRef, Error, BlockHeaderProvider, BlockProvider, BlockOrigin, TransactionMeta, IndexedBlockProvider,
	TransactionMetaProvider, TransactionProvider, TransactionOutputProvider, BlockChain, Store,
	SideChainOrigin, ForkChain, Forkable, CanonStore, ConfigStore, BestBlock, TransactionPrunableData,
	TransactionOutputMeta,
};

const KEY_BEST_BLOCK_NUMBER: &'static str = "best_block_number";
const KEY_BEST_BLOCK_HASH: &'static str = "best_block_hash";
const KEY_CONSENSUS_FORK: &'static str = "consensus_fork";
const KEY_DB_VERSION: &'static str = "db_version";

const MAX_FORK_ROUTE_PRESET: u32 = 2048;

/// Database pruning parameters.
pub struct PruningParams {
	/// Pruning depth (i.e. we're saving pruning_depth last blocks because we believe
	/// that fork still can affect these blocks).
	pub pruning_depth: u32,
	/// Prune blocks and transactions that can not became a part of reorg process.
	pub is_active: bool,
}

pub struct BlockChainDatabase<T> where T: KeyValueDatabase {
	best_block: RwLock<BestBlock>,
	db: T,
	pruning: PruningParams,
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

impl BlockChainDatabase<CacheDatabase<AutoFlushingOverlayDatabase<DiskDatabase>>> {
	pub fn open_at_path<P>(path: P, total_cache: usize) -> Result<Self, Error> where P: AsRef<Path> {
		fs::create_dir_all(path.as_ref()).map_err(|err| Error::DatabaseError(err.to_string()))?;
		let mut cfg = DatabaseConfig::with_columns(Some(COL_COUNT));

		// 'big' columns taking 2/14 of the cache
		let big_column_cache = total_cache * 2 / 14;
		cfg.set_cache(Some(COL_BLOCK_HEADERS), big_column_cache);
		cfg.set_cache(Some(COL_BLOCK_TRANSACTIONS), big_column_cache);
		cfg.set_cache(Some(COL_TRANSACTION_PRUNABLE), big_column_cache);
		cfg.set_cache(Some(COL_TRANSACTION_META), big_column_cache);
		cfg.set_cache(Some(COL_TRANSACTION_OUTPUT), big_column_cache);

		// 'small' columns taking 1/14 of the cache
		let small_column_cache = total_cache * 2 / 14;
		cfg.set_cache(Some(COL_BLOCK_HASHES), small_column_cache);
		cfg.set_cache(Some(COL_BLOCK_NUMBERS), small_column_cache);

		cfg.bloom_filters.insert(Some(COL_TRANSACTION_META), 32);
		cfg.bloom_filters.insert(Some(COL_TRANSACTION_OUTPUT), 32);

		match DiskDatabase::open(cfg, path) {
			Ok(db) => Ok(Self::open_with_cache(db)),
			Err(err) => Err(Error::DatabaseError(err))
		}
	}
}

impl BlockChainDatabase<MemoryDatabase> {
	pub fn init_test_chain(blocks: Vec<IndexedBlock>) -> Self {
		let store = BlockChainDatabase::open(MemoryDatabase::default());

		for block in blocks {
			let hash = block.hash().clone();
			store.insert(block).unwrap();
			store.canonize(&hash).unwrap();
		}
		store
	}
}

impl<T> BlockChainDatabase<CacheDatabase<AutoFlushingOverlayDatabase<T>>> where T: KeyValueDatabase {
	pub fn open_with_cache(db: T) -> Self {
		let db = CacheDatabase::new(AutoFlushingOverlayDatabase::new(db, 50));
		let best_block = Self::read_best_block(&db).unwrap_or_default();
		BlockChainDatabase {
			best_block: RwLock::new(best_block),
			db: db,
			pruning: Default::default(),
		}
	}
}

impl<T> BlockChainDatabase<T> where T: KeyValueDatabase {
	fn read_best_block(db: &T) -> Option<BestBlock> {
		let best_number = db.get(&Key::Meta(KEY_BEST_BLOCK_NUMBER)).map(KeyState::into_option).map(|x| x.and_then(Value::as_meta));
		let best_hash = db.get(&Key::Meta(KEY_BEST_BLOCK_HASH)).map(KeyState::into_option).map(|x| x.and_then(Value::as_meta));

		match (best_number, best_hash) {
			(Ok(None), Ok(None)) => None,
			(Ok(Some(number)), Ok(Some(hash))) => Some(BestBlock {
				number: deserialize(&**number).expect("Inconsistent DB. Invalid best block number."),
				hash: deserialize(&**hash).expect("Inconsistent DB. Invalid best block hash."),
			}),
			_ => panic!("Inconsistent DB"),
		}
	}

	pub fn open(db: T) -> Self {
		let best_block = Self::read_best_block(&db).unwrap_or_default();
		BlockChainDatabase {
			best_block: RwLock::new(best_block),
			db: db,
			pruning: Default::default(),
		}
	}

	pub fn set_pruning_params(&mut self, params: PruningParams) {
		// 11 is the because we take 11 last headers to calculate MTP. This limit must be
		// raised to at least MAX_FORK_ROUTE_PRESET and assert replaced with Err before
		// passing control over this to cli
		// right now it is 11 to speed up tests
		assert!(params.pruning_depth >= 11);

		self.pruning = params;
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

	pub fn insert(&self, block: IndexedBlock) -> Result<(), Error> {
		if self.contains_block(block.hash().clone().into()) {
			return Ok(())
		}

		let parent_hash = block.header.raw.previous_header_hash.clone();
		if !self.contains_block(parent_hash.clone().into()) && !parent_hash.is_zero() {
			return Err(Error::UnknownParent);
		}

		let mut update = DBTransaction::new();
		update.insert(KeyValue::BlockHeader(block.hash().clone(), block.header.raw));
		let tx_hashes = block.transactions.iter().map(|tx| tx.hash.clone()).collect::<Vec<_>>();
		update.insert(KeyValue::BlockTransactions(block.header.hash.clone(), List::from(tx_hashes)));

		for tx in block.transactions {
			// insert transaction outputs
			let total_outputs = tx.raw.outputs.len() as u32;
			for (tx_out_index, tx_out) in tx.raw.outputs.into_iter().enumerate() {
				let tx_out_key = OutPoint { hash: tx.hash.clone(), index: tx_out_index as u32 };
				let tx_out = TransactionOutputMeta::new(tx_out);
				update.insert(KeyValue::TransactionOutput(tx_out_key, tx_out));
			}

			// insert prunable transaction data
			let tx_prunable = TransactionPrunableData {
				version: tx.raw.version,
				lock_time: tx.raw.lock_time,
				inputs: tx.raw.inputs,
				total_outputs,
			};
			update.insert(KeyValue::TransactionPrunable(tx.hash, tx_prunable));
		}

		self.db.write(update).map_err(Error::DatabaseError)
	}

	/// Rollbacks single best block
	fn rollback_best(&self) -> Result<H256, Error> {
		let best_block: BestBlock = self.best_block.read().clone();
		let decanonized = match self.block(best_block.hash.clone().into()) {
			Some(block) => block,
			None => return Ok(H256::default()),
		};
		let decanonized_hash = self.decanonize()?;
		debug_assert_eq!(decanonized.hash(), decanonized_hash);

		// and now remove decanonized block from database
		// all code currently works in assumption that origin of all blocks is one of:
		// {CanonChain, SideChain, SideChainBecomingCanonChain}
		let mut update = DBTransaction::new();
		self.delete_canon_block(best_block.number, &mut update, true);
		self.db.write(update).map_err(Error::DatabaseError)?;

		Ok(self.best_block().hash)
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
		update.insert(KeyValue::BlockHash(new_best_block.number, new_best_block.hash.clone()));
		update.insert(KeyValue::BlockNumber(new_best_block.hash.clone(), new_best_block.number));
		update.insert(KeyValue::Meta(KEY_BEST_BLOCK_HASH, serialize(&new_best_block.hash)));
		update.insert(KeyValue::Meta(KEY_BEST_BLOCK_NUMBER, serialize(&new_best_block.number)));

		let mut overlay = MemoryDatabaseCore::default();
		for (tx_index, tx) in block.transactions.into_iter().enumerate() {
			// insert tx meta
			let is_coinbase = tx_index == 0;
			let tx_meta = TransactionMeta::new(is_coinbase, new_best_block.number, tx.raw.outputs.len() as u32);
			overlay_write(&mut overlay, Operation::Insert(KeyValue::TransactionMeta(tx.hash.clone(), tx_meta)));

			// mark spent outputs as spent
			if !is_coinbase {
				for tx_input in tx.raw.inputs {
					// update transaction meta
					let tx_out_key = tx_input.previous_output;
					let mut tx_meta = overlay_get(&overlay, &self.db, &Key::TransactionMeta(tx_out_key.hash.clone()))
						.and_then(Value::as_transaction_meta)
						.expect("canonized tx is verified; verification includes check that previous outputs exists; meta is never pruned; qed");
					tx_meta.add_spent_output();
					overlay_write(&mut overlay, Operation::Insert(KeyValue::TransactionMeta(tx_out_key.hash.clone(), tx_meta)));

					// update output
					let mut tx_out = overlay_get(&overlay, &self.db, &Key::TransactionOutput(tx_out_key.clone()))
						.and_then(Value::as_transaction_output)
						.expect("canonized tx is verified; verification includes check that previous outputs exists; it is within reorg depth => not pruned; qed");
					tx_out.is_spent = true;
					overlay_write(&mut overlay, Operation::Insert(KeyValue::TransactionOutput(tx_out_key, tx_out)));
				}
			}
		}

		update.operations.extend(overlay.drain_operations());

		self.prune_ancient_block_transactions(&mut update, new_best_block.number);

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
		update.delete(Key::BlockHash(block_number));
		update.delete(Key::BlockNumber(block_hash.clone()));
		update.insert(KeyValue::Meta(KEY_BEST_BLOCK_HASH, serialize(&new_best_block.hash)));
		update.insert(KeyValue::Meta(KEY_BEST_BLOCK_NUMBER, serialize(&new_best_block.number)));

		let mut overlay = MemoryDatabaseCore::default();
		for (tx_index, tx) in block.transactions.into_iter().enumerate().rev() {
			// delete tx meta
			overlay_write(&mut overlay, Operation::Delete(Key::TransactionMeta(tx.hash.clone())));

			// mark spent outputs as non-spent
			let is_coinbase = tx_index == 0;
			if !is_coinbase {
				for tx_input in tx.raw.inputs {
					// update transaction meta
					let tx_out_key = tx_input.previous_output;
					let mut tx_meta = overlay_get(&overlay, &self.db, &Key::TransactionMeta(tx_out_key.hash.clone()))
						.and_then(Value::as_transaction_meta)
						.expect("canonized tx is verified; verification includes check that previous outputs exists; meta is never pruned; qed");
					tx_meta.remove_spent_output();
					overlay_write(&mut overlay, Operation::Insert(KeyValue::TransactionMeta(tx_out_key.hash.clone(), tx_meta)));

					// update output
					let mut tx_out = overlay_get(&overlay, &self.db, &Key::TransactionOutput(tx_out_key.clone()))
						.and_then(Value::as_transaction_output)
						.expect("on non-pruned db: previous outputs of canonized tx are stored in TransactionOutput column forever;\
							on pruned db: previous outputs of canonized tx are stored in TransactionOutput column until decanonization is impossible; qed");
					tx_out.is_spent = false;
					overlay_write(&mut overlay, Operation::Insert(KeyValue::TransactionOutput(tx_out_key, tx_out)));
				}
			}
		}

		update.operations.extend(overlay.drain_operations());

		self.db.write(update).map_err(Error::DatabaseError)?;
		*best_block = new_best_block;
		Ok(block_hash)
	}

	fn get(&self, key: Key) -> Option<Value> {
		self.db.get(&key).expect("db value to be fine").into_option()
	}

	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}

	fn delete_canon_block(&self, block_number: u32, update: &mut DBTransaction, leave_no_traces: bool) -> bool {
		let block_hash = match self.block_hash(block_number) {
			Some(block_hash) => block_hash,
			None => return false,
		};

		if self.get(Key::BlockHeader(block_hash.clone())).is_none() {
			return false;
		}

		update.delete(Key::BlockHeader(block_hash.clone()));
		update.delete(Key::BlockTransactions(block_hash.clone()));
		if leave_no_traces {
			update.delete(Key::BlockHash(block_number));
			update.delete(Key::BlockNumber(block_hash.clone()));

			let block_transactions_hashes = self.block_transaction_hashes(block_hash.clone().into());
			for (tx_hash, tx) in block_transactions_hashes.into_iter().filter_map(|h| self.transaction(&h).map(|t| (h, t))) {
				update.delete(Key::TransactionPrunable(tx_hash.clone()));
				update.delete(Key::TransactionMeta(tx_hash.clone()));
				for tx_out_index in 0..tx.outputs.len() {
					update.delete(Key::TransactionOutput(OutPoint { hash: tx_hash.clone(), index: tx_out_index as u32 }));
				}
			}
		}

		true
	}

	fn prune_ancient_block_transactions(&self, update: &mut DBTransaction, new_best_block_number: u32) {
		if !self.pruning.is_active {
			return;
		}

		let ancient_block = match new_best_block_number.checked_sub(self.pruning.pruning_depth).and_then(|n| self.block_hash(n)) {
			Some(ancient_block) => ancient_block,
			None => return,
		};

		// we can delete: block header (takes almost no space => leave as is) + block transactions
		// for each transaction: prunable data + outputs that it has spent
		let txs = self.block_transactions(ancient_block.clone().into());
		update.delete(Key::BlockTransactions(ancient_block.clone()));
		for tx in txs {
			update.delete(Key::TransactionPrunable(tx.hash()));
			for input in tx.inputs {
				update.delete(Key::TransactionOutput(input.previous_output));
			}
		}
	}
}

impl<T> BlockHeaderProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.block_header(block_ref)
			.map(|header| serialize(&header))
	}

	fn block_header(&self, block_ref: BlockRef) -> Option<BlockHeader> {
		self.resolve_hash(block_ref)
			.and_then(|hash| self.get(Key::BlockHeader(hash)))
			.and_then(Value::as_block_header)
	}
}

impl<T> BlockProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn block_number(&self, hash: &H256) -> Option<u32> {
		self.get(Key::BlockNumber(hash.clone()))
			.and_then(Value::as_block_number)
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		self.get(Key::BlockHash(number))
			.and_then(Value::as_block_hash)
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
		self.block_header(block_ref)
			.is_some()
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		self.resolve_hash(block_ref)
			.and_then(|hash| self.get(Key::BlockTransactions(hash)))
			.and_then(Value::as_block_transactions)
			.map(List::into)
			.unwrap_or_default()
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<Transaction> {
		self.block_transaction_hashes(block_ref)
			.into_iter()
			.filter_map(|t| self.transaction(&t))
			.collect()
	}
}

impl<T> IndexedBlockProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn indexed_block_header(&self, block_ref: BlockRef) -> Option<IndexedBlockHeader> {
		self.resolve_hash(block_ref)
			.and_then(|block_hash| {
				self.block_header(block_hash.clone().into())
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
			.filter_map(|tx_hash| {
				self.transaction(&tx_hash)
					.map(|tx| IndexedTransaction::new(tx_hash, tx))
			})
			.collect()
	}
}

impl<T> TransactionMetaProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		self.get(Key::TransactionMeta(hash.clone()))
			.and_then(Value::as_transaction_meta)
	}
}

impl<T> TransactionProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.transaction(hash)
			.map(|tx| serialize(&tx))
	}

	fn transaction(&self, hash: &H256) -> Option<Transaction> {
		self.get(Key::TransactionPrunable(hash.clone()))
			.and_then(Value::as_transaction_prunable)
			.and_then(|tx| {
				// only return tx if all its outputs are not pruned
				let outputs = (0..tx.total_outputs)
					.filter_map(|index| self.transaction_output(&OutPoint { hash: hash.clone(), index }, 0))
					.collect::<Vec<_>>();
				if outputs.len() != tx.total_outputs as usize {
					return None;
				}

				Some(Transaction {
					version: tx.version,
					inputs: tx.inputs,
					outputs: outputs,
					lock_time: tx.lock_time,
				})
			})
	}
}

impl<T> TransactionOutputProvider for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn transaction_output(&self, prevout: &OutPoint, _transaction_index: usize) -> Option<TransactionOutput> {
		self.get(Key::TransactionOutput(prevout.clone()))
			.and_then(Value::as_transaction_output)
			.map(|o| o.output)
}

	fn is_spent(&self, prevout: &OutPoint) -> bool {
		self.get(Key::TransactionMeta(prevout.hash.clone()))
			.and_then(|_| self.get(Key::TransactionOutput(prevout.clone())))
			.and_then(Value::as_transaction_output)
			.map(|o| o.is_spent)
			.unwrap_or(false)
	}
}

impl<T> BlockChain for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn insert(&self, block: IndexedBlock) -> Result<(), Error> {
		BlockChainDatabase::insert(self, block)
	}

	fn rollback_best(&self) -> Result<H256, Error> {
		BlockChainDatabase::rollback_best(self)
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

impl<T> ConfigStore for BlockChainDatabase<T> where T: KeyValueDatabase {
	fn db_version(&self) -> u8 {
		self.get(Key::Meta(KEY_DB_VERSION))
			.and_then(Value::as_meta)
			.and_then(|v| v.get(0).cloned())
			.unwrap_or_default()
	}

	fn set_db_version(&self, version: u8) -> Result<(), Error> {
		let mut update = DBTransaction::new();
		update.insert(KeyValue::Meta(KEY_DB_VERSION, vec![version].into()));
		self.db.write(update).map_err(Error::DatabaseError)
	}
	
	fn consensus_fork(&self) -> Result<Option<String>, Error> {
		match self.get(Key::Meta(KEY_CONSENSUS_FORK)).and_then(Value::as_meta) {
			Some(consensus_fork) => String::from_utf8(consensus_fork.into())
				.map_err(|e| Error::DatabaseError(format!("{}", e)))
				.map(Some),
			None => Ok(None),
		}
	}

	fn set_consensus_fork(&self, consensus_fork: &str) -> Result<(), Error> {
		let mut update = DBTransaction::new();
		update.insert(KeyValue::Meta(KEY_CONSENSUS_FORK, consensus_fork.as_bytes().into()));
		self.db.write(update).map_err(Error::DatabaseError)
	}
}

impl Default for PruningParams {
	fn default() -> Self {
		PruningParams {
			pruning_depth: MAX_FORK_ROUTE_PRESET,
			is_active: false,
		}
	}
}
