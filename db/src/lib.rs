//! Bitcoin database

extern crate elastic_array;
extern crate rocksdb;
extern crate parking_lot;

#[cfg(test)] extern crate ethcore_devtools as devtools;

pub mod kvdb;
