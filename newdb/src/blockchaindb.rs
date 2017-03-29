use std::io;
use parking_lot::RwLock;
use byteorder::LittleEndian;
use hash::H256;
use bytes::Bytes;
use chain::{IndexedBlock, IndexedBlockHeader, BlockHeader, Block, Transaction};
use ser::{
	deserialize, serialize, serialize_list, Serializable, Deserializable, Reader, Error as ReaderError,
	DeserializableList
};
use kv::{KeyValueDatabase, OverlayDatabase, Transaction as DBTransaction, Location, Value};
use best_block::BestBlock;
use {BlockRef, Error, BlockHeaderProvider, BlockProvider, BlockOrigin};

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

	pub fn fork(&self) -> ForkChainDatabase<T> {
		ForkChainDatabase {
			blockchain: BlockChainDatabase::open(OverlayDatabase::new(&self.db))
		}
	}

	pub fn switch_to_fork(&self, fork: ForkChainDatabase<T>) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		*best_block = fork.blockchain.best_block.read().clone();
		fork.blockchain.db.flush().map_err(Error::DatabaseError)
	}

	pub fn block_origin(&self, header: &IndexedBlockHeader) -> Result<BlockOrigin, Error> {
		let best_block = self.best_block.read();
		if self.contains_block(header.hash().clone().into()) {
			// it does not matter if it's canon chain or side chain block
			return Ok(BlockOrigin::KnownBlock)
		}

		if best_block.hash == header.raw.previous_header_hash {
			return Ok(BlockOrigin::CanonChain)
		}

		if !self.contains_block(header.raw.previous_header_hash.clone().into()) {
			return Err(Error::UnknownParent)
		}

		let mut next_hash = header.raw.previous_header_hash.clone();

		for fork_len in 0..MAX_FORK_ROUTE_PRESET {
			match self.block_number(&next_hash) {
				Some(number) => {
					if number + fork_len >= best_block.number {
						return Ok(BlockOrigin::SideChainBecomingCanonChain)
					} else {
						return Ok(BlockOrigin::SideChain)
					}
				},
				None => {
					next_hash = self.block_header(&next_hash)
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
		let parent_number = match self.block_number(&parent_hash) {
			Some(number) => number,
			None => return Err(Error::UnknownParent),
		};

		let mut update = DBTransaction::new();
		update.insert(COL_BLOCK_HEADERS.into(), block.hash(), &block.header.raw);
		for tx in &block.transactions {
			update.insert(COL_TRANSACTIONS.into(), &tx.hash, &tx.raw);
		}
		let tx_hashes = serialize_list::<H256, &H256>(&block.transactions.iter().map(|tx| &tx.hash).collect::<Vec<_>>());
		update.insert_raw(COL_BLOCK_TRANSACTIONS.into(), &**block.hash(), &tx_hashes);
		update.insert(COL_BLOCK_NUMBERS.into(), block.hash(), &(parent_number + 1));
		self.db.write(update).map_err(Error::DatabaseError)
	}

	/// Marks block as a new best block.
	/// Block must be already inserted into db, and it's parent must be current best block.
	/// Updates meta data.
	pub fn canonize(&self, hash: &H256) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		let block = match self.block(hash.clone().into()) {
			Some(block) => block,
			None => return Err(Error::CannotCanonize),
		};

		if best_block.hash != block.block_header.previous_header_hash {
			return Err(Error::CannotCanonize);
		}

		best_block.hash = hash.clone();
		best_block.number = best_block.number + 1;

		let mut update = DBTransaction::new();
		update.insert(COL_BLOCK_HASHES.into(), &best_block.number, &best_block.hash);
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_HASH, &serialize(&best_block.hash));
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_NUMBER, &serialize(&best_block.number));
		self.db.write(update).map_err(Error::DatabaseError)
	}

	pub fn decanonize(&self) -> Result<(), Error> {
		let mut best_block = self.best_block.write();
		let block = match self.block(best_block.hash.clone().into()) {
			Some(block) => block,
			None => return Err(Error::CannotCanonize),
		};

		best_block.hash = block.block_header.previous_header_hash;
		best_block.number = best_block.number - 1;

		let mut update = DBTransaction::new();
		update.delete(COL_BLOCK_HASHES.into(), &(best_block.number + 1));
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_HASH, &serialize(&best_block.hash));
		update.insert_raw(COL_META.into(), KEY_BEST_BLOCK_NUMBER, &serialize(&best_block.number));
		self.db.write(update).map_err(Error::DatabaseError)
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
		self.resolve_hash(block_ref).and_then(|block_hash| {
			self.get(COL_BLOCK_HEADERS.into(), &block_hash)
				.map(|header| {
					let transactions = self.block_transactions(block_hash.into());
					Block::new(header, transactions)
				})
			})
	}

	fn contains_block(&self, block_ref: BlockRef) -> bool {
		match block_ref {
			BlockRef::Hash(ref hash) => self.block_number(hash).is_some(),
			BlockRef::Number(number) => self.block_hash(number).is_some(),
		}
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
