use kv::{KeyValueDatabase, OverlayDatabase};

pub struct BlockChainDatabase<T> where T: KeyValueDatabase {
	db: T,
}

pub struct ForkChainDatabase<'a, T> where T: 'a + KeyValueDatabase {
	blockchain: BlockChainDatabase<OverlayDatabase<'a, T>>,
}

impl<T> BlockChainDatabase<T> where T: KeyValueDatabase {
	pub fn open(db: T) -> Self {
		BlockChainDatabase {
			db: db,
		}
	}

	pub fn fork(&self) -> ForkChainDatabase<T> {
		ForkChainDatabase {
			blockchain: BlockChainDatabase::open(OverlayDatabase::new(&self.db))
		}
	}

	pub fn switch_to_fork(&self, fork: ForkChainDatabase<T>) -> Result<(), String> {
		fork.blockchain.db.flush()
	}
}
