use chain::IndexedBlock;
use storage::{BlockProvider, BlockRef, BlockOrigin, ForkChain};
use db::BlockChainDatabase;
use test_data;

use super::Benchmark;

pub fn fetch(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 1000;

	benchmark.samples(BLOCKS);

	// test setup
	let genesis: IndexedBlock = test_data::genesis().into();
	let store = BlockChainDatabase::init_test_chain(vec![genesis.clone()]);

	let mut rolling_hash = genesis.hash().clone();
	let mut blocks = Vec::new();
	let mut hashes = Vec::new();

	for x in 0..BLOCKS {
		let next_block = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);
		hashes.push(rolling_hash.clone());
	}

	for block in blocks.into_iter() {
		let block: IndexedBlock = block.into();
		let hash = block.hash().clone();
		store.insert(block).unwrap();
		store.canonize(&hash).unwrap();
	}

	// bench
	benchmark.start();
	for _ in 0..BLOCKS {
		let block = store.block(BlockRef::Hash(hashes[0].clone())).unwrap();
		assert_eq!(block.hash(), &hashes[0]);
	}
	benchmark.stop();
}

pub fn write(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 1000;
	benchmark.samples(BLOCKS);

	// setup
	let genesis: IndexedBlock = test_data::genesis().into();
	let store = BlockChainDatabase::init_test_chain(vec![genesis.clone()]);

	let mut rolling_hash = genesis.hash().clone();

	let mut blocks: Vec<IndexedBlock> = Vec::new();

	for x in 0..BLOCKS {
		let next_block = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block.into());
	}

	// bench
	benchmark.start();
	for block in blocks {
		let hash = block.hash().clone();
		store.insert(block).unwrap();
		store.canonize(&hash).unwrap();
	}
	benchmark.stop();
}

pub fn reorg_short(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 1000;
	benchmark.samples(BLOCKS);

	// setup
	let genesis: IndexedBlock = test_data::genesis().into();
	let store = BlockChainDatabase::init_test_chain(vec![genesis.clone()]);

	let mut rolling_hash = genesis.hash().clone();

	let mut blocks = Vec::new();

	for x in 0..BLOCKS {
		let base = rolling_hash.clone();

		let next_block = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32 * 4).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);

		let next_block_side = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(base).nonce(x as u32 * 4 + 2).build()
			.build();
		let next_base = next_block_side.hash();
		blocks.push(next_block_side);

		let next_block_side_continue = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(next_base).nonce(x as u32 * 4 + 3).build()
			.build();
		blocks.push(next_block_side_continue);

		let next_block_continue = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32 * 4 + 1).build()
			.build();
		rolling_hash = next_block_continue.hash();
		blocks.push(next_block_continue);
	}

	let mut total: usize = 0;
	let mut reorgs: usize = 0;

	// bench
	benchmark.start();
	for idx in 0..BLOCKS {
		total += 1;
		let block: IndexedBlock = blocks[idx].clone().into();
		let hash = block.hash().clone();

		match store.block_origin(&block.header).unwrap() {
			BlockOrigin::KnownBlock => {
				unreachable!();
			},
			BlockOrigin::CanonChain { .. } => {
				store.insert(block).unwrap();
				store.canonize(&hash).unwrap();
			},
			BlockOrigin::SideChain(_origin) => {
				store.insert(block).unwrap();
			},
			BlockOrigin::SideChainBecomingCanonChain(origin) => {
				reorgs += 1;
				let fork = store.fork(origin).unwrap();
				fork.store().insert(block).unwrap();
				fork.store().canonize(&hash).unwrap();
				store.switch_to_fork(fork).unwrap();
			},
		}
	}
	benchmark.stop();

	// reorgs occur twice per iteration except last one where there only one, blocks are inserted with rate 4/iteration
	// so reorgs = total/2 - 1
	assert_eq!(1000, total);
	assert_eq!(499, reorgs);
}

// 1. write 12000 blocks
// 2. write 100 blocks that has 100 transaction each spending outputs from first 1000 blocks
pub fn write_heavy(benchmark: &mut Benchmark) {
	// params
	const BLOCKS_INITIAL: usize = 12000;
	const BLOCKS: usize = 100;
	const TRANSACTIONS: usize = 100;

	benchmark.samples(BLOCKS);

	// test setup
	let genesis: IndexedBlock = test_data::genesis().into();
	let store = BlockChainDatabase::init_test_chain(vec![genesis.clone()]);

	let mut rolling_hash = genesis.hash().clone();
	let mut blocks = Vec::new();
	let mut hashes = Vec::new();

	for x in 0..BLOCKS_INITIAL {
		let next_block = test_data::block_builder()
			.transaction()
				.coinbase()
				.lock_time(x as u32)
				.output().value(5000000000).build()
				.build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);
		hashes.push(rolling_hash.clone());
	}

	for b in 0..BLOCKS {
		let mut builder = test_data::block_builder()
			.transaction().coinbase().build();

		for t in 0..TRANSACTIONS {
			builder = builder.transaction()
				.input().hash(blocks[b*TRANSACTIONS+t].transactions()[0].hash()).build() // default index is 0 which is ok
				.output().value(1000).build()
				.build();
		}

		let next_block = builder.merkled_header().parent(rolling_hash).build().build();

		rolling_hash = next_block.hash();
		blocks.push(next_block);
		hashes.push(rolling_hash.clone());
	}

	for block in blocks[..BLOCKS_INITIAL].iter() {
		let block: IndexedBlock = block.clone().into();
		let hash = block.hash().clone();
		store.insert(block).expect("cannot insert initial block");
		store.canonize(&hash).unwrap();
	}

	// bench
	benchmark.start();
	for block in blocks[BLOCKS_INITIAL..].iter() {
		let block: IndexedBlock = block.clone().into();
		let hash = block.hash().clone();
		store.insert(block).expect("cannot insert bench block");
		store.canonize(&hash).unwrap();
	}
	benchmark.stop();
}
