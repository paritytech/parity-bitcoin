//! Bitcoin database

extern crate elastic_array;
extern crate rocksdb;
extern crate parking_lot;
extern crate primitives;

#[cfg(test)] extern crate ethcore_devtools as devtools;

mod kvdb;

pub type Bytes = Vec<u8>;

pub enum BlockRef {
	Number(u64),
	Hash(primitives::hash::H256),
}
