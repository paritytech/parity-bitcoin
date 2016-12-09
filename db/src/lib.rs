//! Bitcoin database

extern crate elastic_array;
extern crate rocksdb;
extern crate parking_lot;
extern crate primitives;
extern crate byteorder;
extern crate chain;
extern crate serialization;
extern crate bit_vec;
#[macro_use] extern crate log;

#[cfg(test)]
extern crate ethcore_devtools as devtools;
#[cfg(test)]
extern crate test_data;

mod best_block;
mod kvdb;
mod storage;
#[cfg(feature="dev")]
mod test_storage;
mod transaction_meta;
mod block_provider;
mod block_stapler;
mod transaction_provider;
mod transaction_meta_provider;
mod error;
mod update_context;
mod indexed_block;
mod indexed_header;
mod indexed_transaction;

#[derive(Debug, Clone)]
pub enum BlockRef {
	Number(u32),
	Hash(primitives::hash::H256),
}

impl From<u32> for BlockRef {
	fn from(u: u32) -> Self {
		BlockRef::Number(u)
	}
}

impl From<primitives::hash::H256> for BlockRef {
	fn from(hash: primitives::hash::H256) -> Self {
		BlockRef::Hash(hash)
	}
}

#[derive(PartialEq, Debug)]
pub enum BlockLocation {
	Main(u32),
	Side(u32),
}

impl BlockLocation {
	pub fn height(&self) -> u32 {
		match *self {
			BlockLocation::Main(h) | BlockLocation::Side(h) => h,
		}
	}
}

pub type SharedStore = std::sync::Arc<Store + Send + Sync>;

pub use best_block::BestBlock;
pub use storage::{Storage, Store};
pub use error::{Error, ConsistencyError};
pub use kvdb::Database;
pub use transaction_provider::{TransactionProvider, AsTransactionProvider, PreviousTransactionOutputProvider};
pub use transaction_meta_provider::TransactionMetaProvider;
pub use block_stapler::{BlockStapler, BlockInsertedChain};
pub use block_provider::{BlockProvider, BlockHeaderProvider, AsBlockHeaderProvider};
pub use indexed_block::IndexedBlock;
pub use indexed_header::IndexedBlockHeader;
pub use indexed_transaction::IndexedTransaction;

#[cfg(feature="dev")]
pub use test_storage::TestStorage;
