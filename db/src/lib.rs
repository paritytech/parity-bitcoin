//! Bitcoin database

extern crate elastic_array;
extern crate rocksdb;
extern crate parking_lot;
extern crate primitives;
extern crate byteorder;
extern crate chain;
extern crate serialization;

#[cfg(test)]
extern crate ethcore_devtools as devtools;
#[cfg(test)]
extern crate test_data;

mod kvdb;
mod storage;

pub enum BlockRef {
	Number(u64),
	Hash(primitives::hash::H256),
}

pub use storage::{Storage, Store, Error};
pub use kvdb::Database;
