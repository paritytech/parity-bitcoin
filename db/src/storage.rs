//! Bitcoin storage

use std::fs;
use std::path::Path;
use kvdb::{Database, DatabaseConfig};
use byteorder::{LittleEndian, ByteOrder};
use primitives::hash::H256;
use primitives::bytes::Bytes;
use super::{BlockRef, BestBlock, BlockLocation, IndexedBlock};
use serialization::{serialize, deserialize};
use chain;
use parking_lot::RwLock;
use lru_cache::LruCache;

use transaction_meta::TransactionMeta;
use error::{Error, ConsistencyError, MetaError};
use update_context::UpdateContext;
use block_provider::{BlockProvider, BlockHeaderProvider, AsBlockHeaderProvider};
use transaction_provider::TransactionProvider;
use transaction_meta_provider::TransactionMetaProvider;
use block_stapler::{BlockStapler, BlockInsertedChain, Reorganization};

use indexed_header::IndexedBlockHeader;
use indexed_transaction::IndexedTransaction;

pub const COL_COUNT: u32 = 10;
pub const COL_META: u32 = 0;
pub const COL_BLOCK_HASHES: u32 = 1;
pub const COL_BLOCK_HEADERS: u32 = 2;
pub const COL_BLOCK_TRANSACTIONS: u32 = 3;
pub const COL_TRANSACTIONS: u32 = 4;
pub const COL_TRANSACTIONS_META: u32 = 5;
pub const COL_BLOCK_NUMBERS: u32 = 6;
const _COL_RESERVED3: u32 = 7;
const _COL_RESERVED4: u32 = 8;
const _COL_RESERVED5: u32 = 9;
const _COL_RESERVED6: u32 = 10;

const DB_VERSION: u32 = 1;

// TODO: check how bitcoin core deals with long forks
const MAX_FORK_ROUTE_PRESET: usize = 2048;
const TRANSACTION_CACHE_SIZE: usize = 524288;

/// Blockchain storage interface
pub trait Store : BlockProvider + BlockStapler + TransactionProvider + TransactionMetaProvider + AsBlockHeaderProvider {
	/// get best block
	fn best_block(&self) -> Option<BestBlock>;

	/// get best header
	fn best_header(&self) -> Option<chain::BlockHeader>;
}

/// Blockchain storage with rocksdb database
pub struct Storage {
	database: Database,
	best_block: RwLock<Option<BestBlock>>,
	transaction_cache: RwLock<LruCache<H256, chain::Transaction>>,
	meta_cache: RwLock<LruCache<H256, TransactionMeta>>,
}

const KEY_VERSION: &'static[u8] = b"version";
const KEY_BEST_BLOCK_NUMBER: &'static[u8] = b"best_block_number";
const KEY_BEST_BLOCK_HASH: &'static[u8] = b"best_block_hash";

fn u32_key(num: u32) -> [u8; 4] {
	let mut result = [0u8; 4];
	LittleEndian::write_u32(&mut result, num);
	result
}

impl Storage {

	/// new storage at the selected path
	/// if no directory exists, it will be created
	pub fn with_cache<P: AsRef<Path>>(path: P, total_cache: usize) -> Result<Storage, Error> {
		try!(fs::create_dir_all(path.as_ref()));
		let mut cfg = DatabaseConfig::with_columns(Some(COL_COUNT));

		cfg.set_cache(Some(COL_TRANSACTIONS), total_cache / 4);
		cfg.set_cache(Some(COL_TRANSACTIONS_META), total_cache / 4);
		cfg.set_cache(Some(COL_BLOCK_HEADERS), total_cache / 4);

		cfg.set_cache(Some(COL_BLOCK_HASHES), total_cache / 12);
		cfg.set_cache(Some(COL_BLOCK_TRANSACTIONS), total_cache / 12);
		cfg.set_cache(Some(COL_BLOCK_NUMBERS), total_cache / 12);

		let db = try!(Database::open(&cfg, &*path.as_ref().to_string_lossy()));

		let storage = Storage {
			database: db,
			best_block: RwLock::default(),
			transaction_cache: RwLock::new(LruCache::new(TRANSACTION_CACHE_SIZE)),
			meta_cache: RwLock::new(LruCache::new(TRANSACTION_CACHE_SIZE)),
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

	pub fn new<P: AsRef<Path>>(path: P) -> Result<Storage, Error> {
		Self::with_cache(path, 256)
	}

	fn read_meta(&self, key: &[u8]) -> Option<Bytes> {
		self.get(COL_META, key)
	}

	fn read_meta_u32(&self, key: &[u8]) -> Option<u32> {
		self.read_meta(key).map(|val| LittleEndian::read_u32(&val))
	}

	/// get the value of the key in the database
	fn get(&self, col: u32, key: &[u8]) -> Option<Bytes> {
		self.database.get(Some(col), key).expect("fatal db error")
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
			.unwrap_or_else(Bytes::new)
			.chunks(H256::size())
			.map(H256::from)
			.collect()
	}

	fn block_transactions_by_hash(&self, h: &H256) -> Vec<chain::Transaction> {
		self.block_transaction_hashes_by_hash(h)
			.iter()
			.map(|tx_hash| self.transaction(tx_hash).expect("Missing transaction, possible db corruption"))
			.collect()
	}

	fn block_header_by_hash(&self, h: &H256) -> Option<chain::BlockHeader> {
		self.get(COL_BLOCK_HEADERS, &**h).map(|val| {
			deserialize(val.as_ref()).expect("Error deserializing block header, possible db corruption")
		})
	}

	fn block_by_hash(&self, h: &H256) -> Option<IndexedBlock> {
		self.block_header_by_hash(h).map(|header| {
			let tx_hashes = self.block_transaction_hashes_by_hash(h);
			let txs = tx_hashes.into_iter()
				.map(|tx_hash| {
					let tx = self.transaction(&tx_hash).expect("Missing transaction, possible db corruption");
					IndexedTransaction::new(tx_hash, tx)
				})
				.collect::<Vec<_>>();
			IndexedBlock::new(IndexedBlockHeader::new(h.clone(), header), txs)
		})
	}

	/// update transactions metadata in the specified database transaction
	fn update_transactions_meta(&self, context: &mut UpdateContext, number: u32, accepted_txs: &[IndexedTransaction])
		-> Result<(), Error>
	{
		if let Some(tx) = accepted_txs.first() {
			context.meta.insert(
				tx.hash.clone(),
				TransactionMeta::new_coinbase(number, tx.raw.outputs.len())
			);
		}

		for tx in accepted_txs.iter().skip(1) {
			context.meta.insert(
				tx.hash.clone(),
				TransactionMeta::new(number, tx.raw.outputs.len())
			);

			for input in &tx.raw.inputs {
				use std::collections::hash_map::Entry;

				match context.meta.entry(input.previous_output.hash.clone()) {
					Entry::Occupied(mut entry) => {
						let meta = entry.get_mut();
						if meta.is_spent(input.previous_output.index as usize) {
							return Err(Error::double_spend(&input.previous_output.hash));
						}
						meta.denote_used(input.previous_output.index as usize);
					},
					Entry::Vacant(entry) => {
						let mut meta = self.transaction_meta(&input.previous_output.hash)
							.ok_or(Error::unknown_spending(&input.previous_output.hash))?;

						if meta.is_spent(input.previous_output.index as usize) {
							return Err(Error::double_spend(&input.previous_output.hash));
						}

						meta.denote_used(input.previous_output.index as usize);
						entry.insert(meta);
					},
				}
			}
		}

		Ok(())
	}

	/// block decanonization
	///   all transaction outputs used are marked as not used
	///   all transaction meta is removed
	///   DOES NOT update best block
	fn decanonize_block(&self, context: &mut UpdateContext, hash: &H256) -> Result<(), Error> {
		trace!(target: "db", "Decanonizing block {}", hash.to_reversed_str());

		// ensure that block is of the main chain
		try!(self.block_number(hash).ok_or(Error::not_main(hash)));

		// only canonical blocks have numbers, so remove this number entry for the hash
		context.db_transaction.delete(Some(COL_BLOCK_NUMBERS), &**hash);

		// transaction de-provisioning
		let tx_hashes = self.block_transaction_hashes_by_hash(hash);
		for (tx_hash_num, tx_hash) in tx_hashes.iter().enumerate() {
			let tx = self.transaction(tx_hash)
				.expect("Transaction in the saved block should exist as a separate entity indefinitely");

			// remove meta & meta cache
			context.db_transaction.delete(Some(COL_TRANSACTIONS_META), &**tx_hash);
			self.meta_cache.write().remove(&tx_hash);

			// coinbase transaction does not have inputs
			if tx_hash_num == 0 {
				continue;
			}

			// denote outputs as unused
			for input in &tx.inputs {
				use std::collections::hash_map::Entry;
				match context.meta.entry(input.previous_output.hash.clone()) {
					Entry::Occupied(mut entry) => {
						entry.get_mut().denote_unused(input.previous_output.index as usize);
					},
					Entry::Vacant(entry) => {
						let mut meta = self.transaction_meta(&input.previous_output.hash)
							.expect("No transaction metadata! Possible db corruption");
						meta.denote_unused(input.previous_output.index as usize);
						entry.insert(meta);
					},
				}
			}
		}

		Ok(())
	}

	/// Returns the height where the fork occurred and chain up to this place (not including last canonical hash)
	fn fork_route(&self, max_route: usize, hash: &H256) -> Result<(u32, Vec<H256>), Error> {
		let header = try!(self.block_header_by_hash(hash).ok_or(Error::unknown_hash(hash)));

		// only main chain blocks has block numbers
		// so if it has, it is not a fork and we return empty route
		if let Some(number) = self.block_number(hash) {
			return Ok((number, Vec::new()));
		}

		let mut next_hash = header.previous_header_hash;
		let mut result = Vec::new();

		for _ in 0..max_route {
			if let Some(number) = self.block_number(&next_hash) {
				return Ok((number, result));
			}
			result.push(next_hash.clone());
			next_hash = try!(self.block_header_by_hash(&next_hash).ok_or(Error::unknown_hash(hash)))
				.previous_header_hash;
		}
		Err(Error::Consistency(ConsistencyError::ForkTooLong))
	}

	fn best_number(&self) -> Option<u32> {
		self.read_meta_u32(KEY_BEST_BLOCK_NUMBER)
	}

	fn _best_hash(&self) -> Option<H256> {
		self.get(COL_META, KEY_BEST_BLOCK_HASH).map(|val| H256::from(&**val))
	}

	fn canonize_block(&self, context: &mut UpdateContext, at_height: u32, hash: &H256) -> Result<(), Error> {
		trace!(target: "db", "Canonizing block {}", hash.to_reversed_str());

		let block = try!(self.block_by_hash(hash).ok_or(Error::unknown_hash(hash)));
		try!(self.update_transactions_meta(context, at_height, &block.transactions));

		// only canonical blocks are allowed to wield a number
		context.db_transaction.put(Some(COL_BLOCK_HASHES), &u32_key(at_height), &**hash);
		context.db_transaction.write_u32(Some(COL_BLOCK_NUMBERS), &**hash, at_height);

		Ok(())
	}

	// maybe reorganize to the _known_ block
	// it will actually reorganize only when side chain is at least the same length as main
	fn maybe_reorganize(&self, context: &mut UpdateContext, hash: &H256) -> Result<Option<Reorganization>, Error> {
		context.restore_point();

		match self.maybe_reorganize_fallable(context, hash) {
			Ok(result) => Ok(result),
			Err(e) => {
				// todo: log error here
				context.restore();
				warn!("Error while reorganizing to {}: {:?}", hash, e);
				Err(e)
			}
		}
	}

	fn maybe_reorganize_fallable(&self, context: &mut UpdateContext, hash: &H256) -> Result<Option<Reorganization>, Error> {
		if self.block_number(hash).is_some() {
			return Ok(None); // cannot reorganize to canonical block
		}

		// find the route of the block with hash `hash` to the main chain
		let (at_height, route) = try!(self.fork_route(MAX_FORK_ROUTE_PRESET, hash));

		// reorganization is performed only if length of side chain is at least the same as main chain
		// todo: shorter chain may actualy become canonical during difficulty updates, though with rather low probability
		if (route.len() as i32 + 1) < (self.best_number().unwrap_or(0) as i32 - at_height as i32) {
			return Ok(None);
		}

		let mut reorganization = Reorganization::new(at_height);
		let mut now_best = try!(self.best_number().ok_or(Error::Consistency(ConsistencyError::NoBestBlock)));

		// decanonizing main chain to the split point
		loop {
			let next_decanonize = try!(self.block_hash(now_best).ok_or(Error::unknown_number(now_best)));
			try!(self.decanonize_block(context, &next_decanonize));
			reorganization.push_decanonized(&next_decanonize);

			now_best -= 1;

			if now_best == at_height { break; }
		}

		// canonizing all route from the split point
		for new_canonical_hash in &route {
			now_best += 1;
			try!(self.canonize_block(context, now_best, &new_canonical_hash));
			reorganization.push_canonized(&new_canonical_hash);
		}

		// finaly canonizing the top block we are reorganizing to
		try!(self.canonize_block(context, now_best + 1, hash));
		reorganization.push_canonized(&hash);
		reorganization.height = now_best + 1;

		Ok(Some(reorganization))
	}

	/// Decanonizes main chain until best block is the one with `hash`
	pub fn rollback(&self, hash: &H256) -> Result<Vec<H256>, Error> {
		if self.block_number(hash).is_none() {
			return Err(Error::not_main(hash));
		}

		// lock will be held until the end of the routine
		let mut best_block = self.best_block.write();

		let mut context = UpdateContext::new(&self.database, hash);
		let mut result = Vec::new();
		let mut best_number = try!(self.best_number().ok_or(Error::Consistency(ConsistencyError::NoBestBlock)));
		loop {
			let next = try!(self.block_hash(best_number).ok_or(Error::unknown_number(best_number)));
			if &next == hash {
				context.db_transaction.put(Some(COL_META), KEY_BEST_BLOCK_HASH, &**hash);
				context.db_transaction.write_u32(Some(COL_META), KEY_BEST_BLOCK_NUMBER, best_number);
				try!(context.apply(&self.database));
				*best_block = Some(BestBlock { hash: next, number: best_number });

				return Ok(result);
			}
			try!(self.decanonize_block(&mut context, &next));
			best_number -= 1;
			result.push(next);
		}
	}
}

impl BlockHeaderProvider for Storage {
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.resolve_hash(block_ref).and_then(|h| self.get(COL_BLOCK_HEADERS, &*h))
	}

	fn block_header(&self, block_ref: BlockRef) -> Option<chain::BlockHeader> {
		self.block_header_bytes(block_ref).map(
			|bytes| deserialize::<_, chain::BlockHeader>(bytes.as_ref())
				.expect("Error deserializing header, possible db corruption"))
	}
}

impl AsBlockHeaderProvider for Storage {
	fn as_block_header_provider(&self) -> &BlockHeaderProvider {
		&*self
	}
}

impl BlockProvider for Storage {
	fn block_number(&self, hash: &H256) -> Option<u32> {
		self.get(COL_BLOCK_NUMBERS, &**hash)
			.map(|val| LittleEndian::read_u32(&val))
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		self.get(COL_BLOCK_HASHES, &u32_key(number))
			.map(|val| H256::from(&**val))
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transaction_hashes_by_hash(&h))
			.unwrap_or_default()
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
		self.resolve_hash(block_ref)
			.map(|h| self.block_transactions_by_hash(&h))
			.unwrap_or_default()
	}

	fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
		self.resolve_hash(block_ref).and_then(|block_hash|
			self.get(COL_BLOCK_HEADERS, &*block_hash)
				.map(|header_bytes| {
					let transactions = self.block_transactions_by_hash(&block_hash);
					let header = deserialize::<_, chain::BlockHeader>(header_bytes.as_ref())
						.expect("Error deserializing header, possible db corruption");
					chain::Block::new(header, transactions)
			})
		)
	}
}

impl BlockStapler for Storage {

	/// insert pre-processed block in the storage
	fn insert_block(&self, block: &chain::Block) -> Result<BlockInsertedChain, Error> {
		self.insert_indexed_block(&block.clone().into())
	}

	fn insert_indexed_block(&self, block: &IndexedBlock) -> Result<BlockInsertedChain, Error> {
		// ! lock will be held during the entire insert routine
		let mut best_block = self.best_block.write();

		let mut context = UpdateContext::new(&self.database, block.hash());

		let block_hash = block.hash();

		let mut new_best_hash = match best_block.as_ref().map(|bb| &bb.hash) {
			Some(best_hash) if &block.header.raw.previous_header_hash != best_hash => best_hash.clone(),
			_ => block_hash.clone(),
		};

		let mut new_best_number = match best_block.as_ref().map(|b| b.number) {
			Some(best_number) => {
				if block.hash() == &new_best_hash { best_number + 1 }
				else { best_number }
			},
			None => 0,
		};

		let tx_space = block.transactions.len() * 32;
		let mut tx_refs = Vec::with_capacity(tx_space);
		for tx in &block.transactions {
			tx_refs.extend(&*tx.hash);
			context.db_transaction.put(
				Some(COL_TRANSACTIONS),
				&*tx.hash,
				&serialize(&tx.raw),
			);
		}
		context.db_transaction.put(Some(COL_BLOCK_TRANSACTIONS), &**block_hash, &tx_refs);

		context.db_transaction.put(
			Some(COL_BLOCK_HEADERS),
			&**block_hash,
			&serialize(&block.header.raw)
		);

		// the block is continuing the main chain
		let result = if best_block.as_ref().map(|b| b.number) != Some(new_best_number) {
			try!(self.update_transactions_meta(&mut context, new_best_number, &block.transactions));
			context.db_transaction.write_u32(Some(COL_META), KEY_BEST_BLOCK_NUMBER, new_best_number);

			// updating main chain height reference
			context.db_transaction.put(Some(COL_BLOCK_HASHES), &u32_key(new_best_number), &**block_hash);
			context.db_transaction.write_u32(Some(COL_BLOCK_NUMBERS), &**block_hash, new_best_number);

			BlockInsertedChain::Main
		}

		// the block does not continue the main chain
		// but can cause reorganization here
		// this can canonize the block parent if block parent + this block is longer than the main chain
		else {
			match self.maybe_reorganize(&mut context, &block.header.raw.previous_header_hash) {
				Ok(Some(mut reorg)) => {
					// if so, we have new best main chain block
					new_best_number = reorg.height + 1;
					new_best_hash = block_hash.clone();

					// and we canonize it also by provisioning transactions
					try!(self.update_transactions_meta(&mut context, new_best_number, &block.transactions));
					context.db_transaction.write_u32(Some(COL_META), KEY_BEST_BLOCK_NUMBER, new_best_number);
					context.db_transaction.put(Some(COL_BLOCK_HASHES), &u32_key(new_best_number), &*new_best_hash);
					context.db_transaction.write_u32(Some(COL_BLOCK_NUMBERS), &*new_best_hash, new_best_number);

					reorg.push_canonized(&new_best_hash);

					BlockInsertedChain::Reorganized(reorg)
				},
				Err(Error::Consistency(consistency_error)) => {
					match consistency_error {
						ConsistencyError::DoubleSpend(hash) => {
							warn!(
								target: "reorg",
								"Failed to reorganize to {} due to double-spend at {}",
								block_hash.to_reversed_str(),
								hash.to_reversed_str()
							);
							// return without any commit
							return Err(Error::reorganize(&hash));
						},
						ConsistencyError::UnknownSpending(hash) => {
							warn!(
								target: "reorg",
								"Failed to reorganize to {} due to spending unknown transaction {}",
								block_hash.to_reversed_str(),
								hash.to_reversed_str()
							);
							// return without any commit
							return Err(Error::reorganize(&hash));
						},
						ConsistencyError::Unknown(hash) => {
							// this is orphan block inserted or disconnected chain head updated, we allow that (by now)
							// so it is no-op
							warn!(
								target: "reorg",
								"Disconnected chain head {} updated with {}",
								hash.to_reversed_str(),
								block_hash.to_reversed_str()
							);
							BlockInsertedChain::Disconnected

						},
						_ => {
							// we don't allow other errors on side chain/orphans
							return Err(Error::Consistency(consistency_error))
						}
					}
				},
				Err(e) => {
					return Err(e)
				},
				Ok(None) => {
					// reorganize didn't happen but the block is ok, no-op here
					BlockInsertedChain::Side
				}
			}
		};

		// we always update best hash even if it is not changed
		context.db_transaction.put(Some(COL_META), KEY_BEST_BLOCK_HASH, &*new_best_hash);

		// write accumulated transactions meta and update cache
		{
			let mut cache = self.meta_cache.write();
			for (hash, meta) in context.meta.iter() { cache.insert(hash.clone(), meta.clone()); }
		}
		try!(context.apply(&self.database));

		trace!(target: "db", "Best block now ({}, {})", &new_best_hash.to_reversed_str(), &new_best_number);
		// updating locked best block
		*best_block = Some(BestBlock { hash: new_best_hash, number: new_best_number });

		Ok(result)
	}

	fn accepted_location(&self, header: &chain::BlockHeader) -> Option<BlockLocation> {
		let best_number = match self.best_block() {
			None => { return Some(BlockLocation::Main(0)); },
			Some(best) => best.number,
		};

		if let Some(height) = self.block_number(&header.previous_header_hash) {
			if best_number == height { Some(BlockLocation::Main(height + 1)) }
			else { Some(BlockLocation::Side(height + 1)) }
		}
		else {
			match self.fork_route(MAX_FORK_ROUTE_PRESET, &header.previous_header_hash) {
				Ok((height, route)) => {
					// +2 = +1 for parent (fork_route won't include it in route), +1 for self
					Some(BlockLocation::Side(height + route.len() as u32 + 2))
				},
				// possibly that block is totally unknown
				_ => None,
			}
		}
	}
}

impl TransactionProvider for Storage {

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.get(COL_TRANSACTIONS, &**hash)
	}

	fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
		let mut cache = self.transaction_cache.write();

		let (tx, is_cached) = {
			let cached_transaction = cache.get_mut(hash);
			match cached_transaction {
				None => {
					(
						self.transaction_bytes(hash).map(|tx_bytes| {
							let tx: chain::Transaction = deserialize(tx_bytes.as_ref())
								.expect("Failed to deserialize transaction: db corrupted?");
							tx
						}),
						false
					)
				},
				Some(tx) => (Some(tx.clone()), true)
			}
		};

		match tx {
			Some(ref tx) => { if !is_cached { cache.insert(hash.clone(), tx.clone()); } }
			None => {}
		};

		tx
	}
}

impl TransactionMetaProvider for Storage {

	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		let mut cache = self.meta_cache.write();

		let (meta, is_cached) = {
			let cached_meta = cache.get_mut(hash);
			match cached_meta {
				None => {
					(self.get(COL_TRANSACTIONS_META, &**hash).map(|val|
						TransactionMeta::from_bytes(&val).expect("Invalid transaction metadata: db corrupted?")
					), false)
				},
				Some(meta) => (Some(meta.clone()), true)
			}
		};

		match meta {
			Some(ref meta) => { if !is_cached { cache.insert(hash.clone(), meta.clone()); } }
			None => {}
		};

		meta
	}
}

impl Store for Storage {
	fn best_block(&self) -> Option<BestBlock> {
		self.best_block.read().clone()
	}

	fn best_header(&self) -> Option<chain::BlockHeader> {
		self.best_block.read().as_ref().and_then(
			|bb| Some(self.block_header_by_hash(&bb.hash).expect("Best block exists but no such header. Race condition?")),
		)
	}
}

#[cfg(test)]
mod tests {

	use block_provider::BlockProvider;
	use block_stapler::{BlockStapler, BlockInsertedChain};
	use transaction_meta_provider::TransactionMetaProvider;
	use transaction_provider::TransactionProvider;
	use update_context::UpdateContext;
	use error::{ConsistencyError, Error};

	use super::{Storage, Store};
	use devtools::RandomTempPath;
	use chain::Block;
	use super::super::{BlockRef, BlockLocation};
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
	fn reorganize_simple() {
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

		let (_, side_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(2)
					.build()
				.build()
			.build();

		store.insert_block(&side_block1).expect("side block 1 should insert with no problems");

		// chain should not reorganize to side_block1
		assert_eq!(store.best_block().unwrap().hash, main_hash1);
	}

	#[test]
	fn fork_smoky() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let (_, main_block1) = test_data::block_hash_builder()
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

	#[test]
	fn fork_long() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();

		let mut last_main_block_hash = genesis.hash();
		let mut last_side_block_hash = genesis.hash();

		for n in 0..32 {
			let (new_main_hash, main_block) = test_data::block_hash_builder()
				.block()
					.header().parent(last_main_block_hash)
						.nonce(n*2)
						.build()
					.build()
				.build();
			store.insert_block(&main_block).expect(&format!("main block {} should insert with no problems", n));
			last_main_block_hash = new_main_hash;

			let (new_side_hash, side_block) = test_data::block_hash_builder()
				.block()
					.header().parent(last_side_block_hash)
						.nonce(n*2 + 1)
						.build()
					.build()
				.build();
			store.insert_block(&side_block).expect(&format!("side block {} should insert with no problems", n));
			last_side_block_hash = new_side_hash;
		}

		let (height, route) = store.fork_route(128, &last_side_block_hash).unwrap();
		assert_eq!(height, 0);
		assert_eq!(route.len(), 31);

		let (reorg_side_hash, reorg_side_block) = test_data::block_hash_builder()
			.block()
				.header().parent(last_side_block_hash)
					.nonce(3)
					.build()
				.build()
			.build();
		store.insert_block(&reorg_side_block).expect("last side block should insert with no problems");

		// store should reorganize to side hash 2, because it represents the longer chain
		assert_eq!(store.best_block().unwrap().hash, reorg_side_hash);
	}

	#[test]
	fn fork_transactions() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::genesis();
		store.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		// Having 2 blocks initially in the main chain

		let block1 = test_data::block_builder()
			.header()
				.nonce(10)
				.parent(genesis.hash())
				.build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(1).build()
				.output().value(3).build()
				.output().value(5).build()
				.output().value(7).build()
				.output().value(9).build()
				.build()
			.build();
		store.insert_block(&block1).expect("Block #2 should get inserted with no error");
		let parent_tx = block1.transactions()[1].hash();

		let block2 = test_data::block_builder()
			.header()
				.nonce(20)
				.parent(block1.hash())
				.build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(parent_tx.clone()).index(1).build()
				.input().hash(parent_tx.clone()).index(3).build()
				.output().value(10).build()
				.build()
			.build();
		store.insert_block(&block2).expect("Block #2 should get inserted with no error");

		// Reorganizing to side chain adding two blocks to it

		let side_block2 = test_data::block_builder()
			.header().parent(block1.hash())
				.nonce(30)
				.build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(parent_tx.clone()).index(0).build()
				.input().hash(parent_tx.clone()).index(2).build()
				.output().value(6).build()
				.build()
			.build();
		store.insert_block(&side_block2).expect("Side block #2 should get inserted with no error");

		let side_block3 = test_data::block_builder()
			.header().parent(side_block2.hash())
				.nonce(40)
				.build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(parent_tx.clone()).index(1).build()
				.output().value(3).build()
				.build()
			.build();
		store.insert_block(&side_block3).expect("Side block #3 should get inserted with no error");

		let meta = store.transaction_meta(&parent_tx).expect("Transaction meta from block # 1 should exist");
		// outputs 0, 1, 2 should be spent in the side chain branch
		// we reorganized to the side chain branch
		// so, outputs 0, 1, 2 should  be spent
		assert!(meta.is_spent(0));
		assert!(meta.is_spent(1));
		assert!(meta.is_spent(2));

		// outputs 3, 4 should not be spent in the side chain branch
		// we reorganized to the side chain branch
		// so, outputs 3, 4 should not be spent
		assert!(!meta.is_spent(3));
		assert!(!meta.is_spent(4));

		// Reorganizing back to main chain with 2 blocks in a row

		let block3 = test_data::block_builder()
			.header().parent(block2.hash())
				.nonce(50)
				.build()
			.transaction().coinbase().build()
			.build();
		store.insert_block(&block3).expect("Block #3 should get inserted with no error");

		let block4 = test_data::block_builder()
			.header().parent(block3.hash())
				.nonce(60)
				.build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(parent_tx.clone()).index(4).build()
				.output().value(9).build()
				.build()
			.build();
		store.insert_block(&block4).expect("Block #4 should get inserted with no error");

		let meta = store.transaction_meta(&parent_tx).expect("Transaction meta from block # 1 should exist");
		// outputs 1, 3, 4 should be spent in the main branch
		// we reorganized to the main branch again after reorganized to side branch
		// so, outputs 1, 3, 4 should be spent
		assert!(meta.is_spent(1));
		assert!(meta.is_spent(3));
		assert!(meta.is_spent(4));

		// outputs 0, 2 should not be spent in the main branch
		// we reorganized to the main branch again after reorganized to side branch
		// so, outputs 0, 2 should not be spent
		assert!(!meta.is_spent(0));
		assert!(!meta.is_spent(2));
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
		let block_hash = block.hash();

		store.insert_block(&block).expect("inserting first block in the decanonize test should not fail");

		let genesis_meta = store.transaction_meta(&genesis_coinbase)
			.expect("Transaction meta for the genesis coinbase transaction should exist");
		assert!(genesis_meta.is_spent(0), "Genesis coinbase should be recorded as spent because block#1 transaction spends it");

		let mut update_context = UpdateContext::new(&store.database, &block_hash);
		store.decanonize_block(&mut update_context, &block_hash)
			.expect("Decanonizing block #1 which was just inserted should not fail");
		update_context.apply(&store.database).unwrap();
		store.meta_cache.write().clear();

		let genesis_meta = store.transaction_meta(&genesis_coinbase)
			.expect("Transaction meta for the genesis coinbase transaction should exist");
		assert!(!genesis_meta.is_spent(0), "Genesis coinbase should be recorded as unspent because we retracted block #1");

		assert_eq!(store.block_number(&block_hash), None);
	}

	#[test]
	fn chain_for_genesis() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let inserted_chain = store.insert_block(&test_data::genesis()).unwrap();

		assert_eq!(inserted_chain, BlockInsertedChain::Main, "Genesis should become main chain");
	}

	#[test]
	fn chain_for_main() {
		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		store.insert_block(&test_data::genesis())
			.expect("Genesis should be inserted with no issues");

		let inserted_chain = store.insert_block(&test_data::block_h1()).unwrap();

		assert_eq!(inserted_chain, BlockInsertedChain::Main, "h1 should become main chain");
	}

	#[test]
	fn chain_for_side() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		store.insert_block(&test_data::genesis())
			.expect("Genesis should be inserted with no issues");

		let block1 = test_data::block_h1();
		let block1_hash = block1.hash();
		store.insert_block(&block1)
			.expect("Block 1 should be inserted with no issues");

		store.insert_block(&test_data::block_h2())
			.expect("Block 2 should be inserted with no issues");

		let block2_side = test_data::block_builder()
			.header().parent(block1_hash).build()
			.build();

		let inserted_chain = store.insert_block(&block2_side).unwrap();

		assert_eq!(inserted_chain, BlockInsertedChain::Side, "h1 should become main chain");
	}

	#[test]
	fn accepted_location_for_genesis() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		let location = store.accepted_location(test_data::genesis().header());

		assert_eq!(Some(BlockLocation::Main(0)), location);
	}


	#[test]
	fn accepted_location_for_main() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		store.insert_block(&test_data::genesis())
			.expect("Genesis should be inserted with no issues in the accepted location test");

		let location = store.accepted_location(test_data::block_h1().header());

		assert_eq!(Some(BlockLocation::Main(1)), location);
	}

	#[test]
	fn accepted_location_for_branch() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		store.insert_block(&test_data::genesis())
			.expect("Genesis should be inserted with no issues in the accepted location test");

		let block1 = test_data::block_h1();
		let block1_hash = block1.hash();
		store.insert_block(&block1)
			.expect("Block 1 should be inserted with no issues in the accepted location test");

		store.insert_block(&test_data::block_h2())
			.expect("Block 2 should be inserted with no issues in the accepted location test");

		let block2_side = test_data::block_builder()
			.header().parent(block1_hash).build()
			.build();

		let location = store.accepted_location(block2_side.header());

		assert_eq!(Some(BlockLocation::Side(2)), location);
	}

	#[test]
	fn accepted_location_for_unknown() {

		let path = RandomTempPath::create_dir();
		let store = Storage::new(path.as_path()).unwrap();

		store.insert_block(&test_data::genesis())
			.expect("Genesis should be inserted with no issues in the accepted location test");

		let location = store.accepted_location(test_data::block_h2().header());

		assert_eq!(None, location);
	}

	#[test]
	fn double_spend() {
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

		store.insert_block(&block).expect("inserting first block in the double spend test should not fail");

		let _dup_block = test_data::block_builder()
			.header().parent(block.hash()).build()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.build()
			.build();

		let insert_result = store.insert_block(&block);
		if insert_result.is_ok() { panic!("Insert should fail because of the double-spend"); }

		let _ = insert_result.map_err(|e| {
			let should_be = ConsistencyError::DoubleSpend(genesis_coinbase);

			if let Error::Consistency(consistency_error) = e {
				assert_eq!(should_be, consistency_error, "Store should return double spend consistency return");
			}
			else { panic!("Insert should fail because of the double-spend"); }
		});
	}

	#[test]
	fn rollback() {
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

		let (main_hash2, main_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(main_hash1.clone())
					.nonce(2)
					.build()
				.build()
			.build();
		store.insert_block(&main_block2).expect("main block 2 should insert with no problems");
		assert_eq!(store.best_block().unwrap().hash, main_hash2);

		store.rollback(&main_hash1).unwrap();

		assert_eq!(store.best_block().unwrap().hash, main_hash1);
	}

	#[test]
	fn fork_route() {
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

		let (main_hash2, main_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(main_hash1)
					.nonce(2)
					.build()
				.build()
			.build();
		store.insert_block(&main_block2).expect("main block 2 should insert with no problems");

		let (main_hash3, main_block3) = test_data::block_hash_builder()
			.block()
				.header().parent(main_hash2)
					.nonce(3)
					.build()
				.build()
			.build();
		store.insert_block(&main_block3).expect("main block 3 should insert with no problems");

		let (_, main_block4) = test_data::block_hash_builder()
			.block()
				.header().parent(main_hash3)
					.nonce(4)
					.build()
				.build()
			.build();
		store.insert_block(&main_block4).expect("main block 4 should insert with no problems");

		let (side_hash1, side_block1) = test_data::block_hash_builder()
			.block()
				.header().parent(genesis.hash())
					.nonce(5)
					.build()
				.build()
			.build();
		store.insert_block(&side_block1).expect("side block 1 should insert with no problems");

		let (side_hash2, side_block2) = test_data::block_hash_builder()
			.block()
				.header().parent(side_hash1.clone())
					.nonce(6)
					.build()
				.build()
			.build();
		store.insert_block(&side_block2).expect("side block 2 should insert with no problems");

		let (side_hash3, side_block3) = test_data::block_hash_builder()
			.block()
				.header().parent(side_hash2.clone())
					.nonce(7)
					.build()
				.build()
			.build();
		store.insert_block(&side_block3).expect("side block 3 should insert with no problems");

		let (h, route) = store.fork_route(16, &side_hash3).expect("Fork route should have been built");

		assert_eq!(h, 0);
		assert_eq!(route, vec![side_hash2, side_hash1]);
	}
}
