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

mod kvdb;
mod storage;

pub type Bytes = Vec<u8>;

pub enum BlockRef {
	Number(u64),
	Hash(primitives::hash::H256),
}

pub use storage::{Storage, Store};
pub use kvdb::Database;
