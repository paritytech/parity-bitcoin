//! Bitcoin database

extern crate elastic_array;
extern crate rocksdb;
extern crate parking_lot;
extern crate primitives;
extern crate byteorder;
extern crate chain;
extern crate serialization;
extern crate bit_vec;

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

pub enum BlockRef {
	Number(u32),
	Hash(primitives::hash::H256),
}

pub use best_block::BestBlock;
pub use storage::{Storage, Store, Error};
pub use kvdb::Database;

#[cfg(feature="dev")]
pub use test_storage::TestStorage;
