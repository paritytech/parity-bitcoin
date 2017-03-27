use parking_lot::RwLock;
use byteorder::LittleEndian;
use hash::H256;
use ser::deserialize;
use kv::{KeyValueDatabase, OverlayDatabase};
use best_block::BestBlock;
use chain::{IndexedBlock, IndexedBlockHeader};
use {Error};

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

	pub fn block_insert_location(&self, header: &IndexedBlockHeader) -> Result<(), Error> {
		unimplemented!();
	}

	pub fn insert(&self, block: &IndexedBlock) -> Result<(), Error> {
		unimplemented!();
	}

	pub fn decanonize(&self, hash: &H256) -> Result<(), Error> {
		unimplemented!();
	}
}
