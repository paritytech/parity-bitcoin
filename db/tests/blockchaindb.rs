extern crate chain;
extern crate storage;
extern crate db;
extern crate test_data;

use chain::IndexedBlock;
use storage::{ForkChain, BlockProvider, BlockHeaderProvider, SideChainOrigin,
	TransactionProvider, TransactionMetaProvider};
use db::{BlockChainDatabase, PruningParams};
use db::kv::{MemoryDatabase, SharedMemoryDatabase};

#[test]
fn insert_block() {
	let store = BlockChainDatabase::open(MemoryDatabase::default());
	let b0: IndexedBlock = test_data::block_h0().into();
	let b1: IndexedBlock = test_data::block_h1().into();
	let b2: IndexedBlock = test_data::block_h2().into();

	store.insert(b0.clone()).unwrap();
	store.insert(b1.clone()).unwrap();
	store.insert(b2.clone()).unwrap();

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

	let decanonized = store.decanonize().unwrap();
	assert_eq!(b2.hash(), &decanonized);
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
		store.insert(b0.clone()).unwrap();
		store.insert(b1.clone()).unwrap();
		store.insert(b2.clone()).unwrap();

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

#[test]
fn switch_to_simple_fork() {
	let store = BlockChainDatabase::open(MemoryDatabase::default());
	let b0: IndexedBlock = test_data::block_h0().into();
	let b1: IndexedBlock = test_data::block_h1().into();
	let b2: IndexedBlock = test_data::block_h2().into();

	store.insert(b0.clone()).unwrap();
	store.insert(b1.clone()).unwrap();
	store.insert(b2.clone()).unwrap();

	store.canonize(b0.hash()).unwrap();
	store.canonize(b1.hash()).unwrap();

	assert_eq!(1, store.best_block().number);
	assert_eq!(b1.hash(), &store.best_block().hash);

	let side_chain_origin = SideChainOrigin {
		ancestor: 1,
		canonized_route: Vec::new(),
		decanonized_route: Vec::new(),
		block_number: 2,
	};

	let fork = store.fork(side_chain_origin).unwrap();
	assert_eq!(1, fork.store().best_block().number);
	assert_eq!(b1.hash(), &fork.store().best_block().hash);

	fork.store().canonize(b2.hash()).unwrap();
	store.switch_to_fork(fork).unwrap();

	assert_eq!(2, store.best_block().number);
	assert_eq!(b2.hash(), &store.best_block().hash);

	let side_chain_origin = SideChainOrigin {
		ancestor: 1,
		canonized_route: Vec::new(),
		decanonized_route: vec![b2.hash().clone()],
		block_number: 2,
	};

	let fork = store.fork(side_chain_origin).unwrap();
	let decanonized = fork.store().decanonize().unwrap();
	assert_eq!(b1.hash(), &decanonized);

	assert_eq!(0, fork.store().best_block().number);
	assert_eq!(b0.hash(), &fork.store().best_block().hash);

	assert_eq!(2, store.best_block().number);
	assert_eq!(b2.hash(), &store.best_block().hash);
	assert_eq!(store.best_block().hash, store.block_hash(2).unwrap());

}

#[test]
fn ancient_blocks_are_pruned() {
	let pruning_depth = 11;
	let blocks = test_data::build_n_empty_blocks_from_genesis(pruning_depth + 2, 1);
	let mut store = BlockChainDatabase::open(MemoryDatabase::default());
	store.set_pruning_params(PruningParams {
		pruning_depth,
		prune_ancient_blocks: true,
		prune_spent_transactions: false,
	});

	store.insert(test_data::genesis().into()).unwrap();
	store.canonize(&test_data::genesis().hash()).unwrap();

	// insert first pruning_depth-1 blocks
	for i in 0..pruning_depth-1 {
		store.insert(blocks[i as usize].clone().into()).unwrap();
		store.canonize(&blocks[i as usize].hash()).unwrap();
	}

	// there are now pruning_depth blocks in the database
	// no blocks are pruned until there are pruning_depth+1
	// => check that nothing is missing
	assert!((0..pruning_depth).all(|i| store.block_header(i.into()).is_some()));

	// insert block# pruning_depth+1
	store.insert(blocks[pruning_depth as usize - 1].clone().into()).unwrap();
	store.canonize(&blocks[pruning_depth as usize - 1].hash()).unwrap();

	// no blocks are pruned, because we never prune genesis block
	assert!((0..pruning_depth + 1).all(|i| store.block_header(i.into()).is_some()));

	// insert block# pruning_depth+2
	store.insert(blocks[pruning_depth as usize].clone().into()).unwrap();
	store.canonize(&blocks[pruning_depth as usize].hash()).unwrap();

	// check that block#1 is pruned and nothing else is missed
	assert!(store.block_header(1.into()).is_none());
	assert!((0..pruning_depth + 2)
		.filter(|i| *i != 1)
		.all(|i| store.block_header(i.into()).is_some()));

	// insert block# pruning_depth+3
	store.insert(blocks[pruning_depth as usize + 1].clone().into()).unwrap();
	store.canonize(&blocks[pruning_depth as usize + 1].hash()).unwrap();

	// check that block#1 and block#2 are pruned and nothing else is missed
	assert!(store.block_header(1.into()).is_none());
	assert!(store.block_header(2.into()).is_none());
	assert!((0..pruning_depth + 3)
		.filter(|i| *i != 1 && *i != 2)
		.all(|i| store.block_header(i.into()).is_some()));
}

#[test]
fn spent_transactions_are_pruned() {
	let pruning_depth = 11;
	let mut store = BlockChainDatabase::open(MemoryDatabase::default());
	store.set_pruning_params(PruningParams {
		pruning_depth,
		prune_ancient_blocks: false,
		prune_spent_transactions: true,
	});

	// insert genesis
	store.insert(test_data::genesis().into()).unwrap();
	store.canonize(&test_data::genesis().hash()).unwrap();

	// insert block with TX we're going to prune later
	// genesis -> tx
	let block_with_input_tx = test_data::block_builder()
		.header().nonce(1).parent(test_data::genesis().hash()).build()
		.transaction().coinbase().build()
		.transaction()
			.output().value(10).build()
			.output().value(20).build()
			.build()
		.build();
	let tx = block_with_input_tx.transactions[1].hash();

	store.insert(block_with_input_tx.clone().into()).unwrap();
	store.canonize(&block_with_input_tx.hash()).unwrap();

	// now insert pruning_depth + 1 empty blocks and check that tx is not pruned
	// genesis -> tx -> 2049 x empty blocks
	let blocks = test_data::build_n_empty_blocks_from(pruning_depth + 1, 2, &block_with_input_tx.header());
	for block in &blocks {
		store.insert(block.clone().into()).unwrap();
		store.canonize(&block.hash()).unwrap();
	}
	assert!(store.transaction(&tx).is_some());
	assert!(store.transaction_meta(&tx).is_some());

	// now insert block with tx, spending out1 of initial tx
	// genesis -> tx -> 2049 x empty blocks -> spend1
	let spending1 = test_data::block_builder()
		.header().nonce(1).parent(blocks.last().unwrap().hash()).build()
		.transaction().coinbase().build()
		.transaction()
			.input().hash(tx.clone()).index(0).build()
			.output().value(10).build()
			.build()
		.build();
	store.insert(spending1.clone().into()).unwrap();
	store.canonize(&spending1.hash()).unwrap();

	// now insert pruning_depth + 1 empty blocks and check that tx is not pruned
	// genesis -> tx -> 2049 x empty -> spend1 -> 2049 x empty
	let blocks = test_data::build_n_empty_blocks_from(pruning_depth + 1, 2, &spending1.header());
	for block in &blocks {
		store.insert(block.clone().into()).unwrap();
		store.canonize(&block.hash()).unwrap();
	}
	assert!(store.transaction(&tx).is_some());
	assert!(store.transaction_meta(&tx).is_some());

	// now insert block with tx, spending out2 of initial tx
	// genesis -> tx -> 2049 x empty -> spend1 -> 2049 x empty -> spend2
	let spending2 = test_data::block_builder()
		.header().nonce(1).parent(blocks.last().unwrap().hash()).build()
		.transaction().coinbase().build()
		.transaction()
			.input().hash(tx.clone()).index(1).build()
			.output().value(20).build()
			.build()
		.build();
	store.insert(spending2.clone().into()).unwrap();
	store.canonize(&spending2.hash()).unwrap();

	// now insert pruning_depth empty blocks and check that tx is not pruned
	// genesis -> tx -> 2049 x empty -> spend1 -> 2049 x empty -> spend2 -> 2048 empty
	let blocks = test_data::build_n_empty_blocks_from(pruning_depth, 2, &spending2.header());
	for block in &blocks {
		store.insert(block.clone().into()).unwrap();
		store.canonize(&block.hash()).unwrap();
	}
	assert!(store.transaction(&tx).is_some());
	assert!(store.transaction_meta(&tx).is_some());

	// and now insert last block && check that tx is pruned
	let blocks = test_data::build_n_empty_blocks_from(1, 2, &blocks.last().unwrap().header());
	for block in &blocks {
		store.insert(block.clone().into()).unwrap();
		store.canonize(&block.hash()).unwrap();
	}
	assert!(store.transaction(&tx).is_none());
	assert!(store.transaction_meta(&tx).is_none());
}
