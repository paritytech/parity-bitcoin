extern crate chain;
extern crate newdb;
extern crate test_data;

use chain::IndexedBlock;
use newdb::kv::{MemoryDatabase, SharedMemoryDatabase};
use newdb::{BlockChainDatabase, BlockProvider};

#[test]
fn insert_block() {
	let store = BlockChainDatabase::open(MemoryDatabase::default());
	let b0: IndexedBlock = test_data::block_h0().into();
	let b1: IndexedBlock = test_data::block_h1().into();
	let b2: IndexedBlock = test_data::block_h2().into();

	store.insert(&b0).unwrap();
	store.insert(&b1).unwrap();
	store.insert(&b2).unwrap();

	assert_eq!(0, store.best_block().number);
	assert!(store.best_block().hash.is_zero());

	store.canonize(b0.hash()).unwrap();
	assert_eq!(0, store.best_block().number);
	assert_eq!(b0.hash(), &store.best_block().hash);

	store.canonize(b1.hash()).unwrap();
	assert_eq!(1, store.best_block().number);
	assert_eq!(b1.hash(), &store.best_block().hash);

	store.canonize(b2.hash()).unwrap();
	assert_eq!(2, store.best_block().number);
	assert_eq!(b2.hash(), &store.best_block().hash);

	store.decanonize().unwrap();
	assert_eq!(1, store.best_block().number);
	assert_eq!(b1.hash(), &store.best_block().hash);

	assert_eq!(b0.hash(), &store.block_hash(0).unwrap());
	assert_eq!(b1.hash(), &store.block_hash(1).unwrap());
	assert!(store.block_hash(2).is_none());

	assert_eq!(0, store.block_number(b0.hash()).unwrap());
	assert_eq!(1, store.block_number(b1.hash()).unwrap());
	assert!(store.block_number(b2.hash()).is_none());
}

#[test]
fn reopen_db() {
	let shared_database = SharedMemoryDatabase::default();
	let b0: IndexedBlock = test_data::block_h0().into();
	let b1: IndexedBlock = test_data::block_h1().into();
	let b2: IndexedBlock = test_data::block_h2().into();

	{
		let store = BlockChainDatabase::open(shared_database.clone());
		store.insert(&b0).unwrap();
		store.insert(&b1).unwrap();
		store.insert(&b2).unwrap();

		store.canonize(b0.hash()).unwrap();
		store.canonize(b1.hash()).unwrap();
		store.canonize(b2.hash()).unwrap();

		store.decanonize().unwrap();
	}
	{
		let store = BlockChainDatabase::open(shared_database);
		assert_eq!(b0.hash(), &store.block_hash(0).unwrap());
		assert_eq!(1, store.best_block().number);
		assert_eq!(b1.hash(), &store.best_block().hash);
	}
}