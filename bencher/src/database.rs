use devtools::RandomTempPath;
use db::{Storage, BlockStapler, BlockProvider, BlockRef, BlockInsertedChain};
use test_data;

use super::Benchmark;

pub fn fetch(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 10000;

	// test setup
	let path = RandomTempPath::create_dir();
	let store = Storage::new(path.as_path()).unwrap();

	let genesis = test_data::genesis();
	store.insert_block(&genesis).unwrap();

	let genesis = test_data::genesis();
	store.insert_block(&genesis).unwrap();

	let mut rolling_hash = genesis.hash();
	let mut blocks = Vec::new();
	let mut hashes = Vec::new();

	for x in 0..BLOCKS {
		let next_block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);
		hashes.push(rolling_hash.clone());
	}

	for block in blocks.iter() { store.insert_block(block).unwrap(); }

	// bench
	benchmark.start();
	for _ in 0..BLOCKS {
		let block = store.block(BlockRef::Hash(hashes[0].clone())).unwrap();
		assert_eq!(&block.hash(), &hashes[0]);
	}
	benchmark.stop();
}

pub fn write(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 1000;

	// setup
	let path = RandomTempPath::create_dir();
	let store = Storage::new(path.as_path()).unwrap();

	let genesis = test_data::genesis();
	store.insert_block(&genesis).unwrap();

	let mut rolling_hash = genesis.hash();

	let mut blocks = Vec::new();

	for x in 0..BLOCKS {
		let next_block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x as u32).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);
	}

	// bench
	benchmark.start();
	for idx in 0..BLOCKS {
		store.insert_block(&blocks[idx]).unwrap();
	}
	benchmark.stop();
}

pub fn reorg_short(benchmark: &mut Benchmark) {
	// params
	const BLOCKS: usize = 1000;

	// setup
	let path = RandomTempPath::create_dir();
	let store = Storage::new(path.as_path()).unwrap();

	let genesis = test_data::genesis();
	store.insert_block(&genesis).unwrap();

	let mut rolling_hash = genesis.hash();

	let mut blocks = Vec::new();

	for x in 0..1000 {
		let base = rolling_hash.clone();

		let next_block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x*4).build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block);

		let next_block_side = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(base).nonce(x * 4 + 2).build()
			.build();
		let next_base = next_block_side.hash();
		blocks.push(next_block_side);

		let next_block_side_continue = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(next_base).nonce(x * 4 + 3).build()
			.build();
		blocks.push(next_block_side_continue);

		let next_block_continue = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction().output().value(5000000000).build().build()
			.merkled_header().parent(rolling_hash.clone()).nonce(x * 4+1).build()
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
		if let BlockInsertedChain::Reorganized(_) = store.insert_block(&blocks[idx]).unwrap() {
			reorgs += 1;
		}
	}
	benchmark.stop();

	assert_eq!(1000, total);
	assert_eq!(499, reorgs);
}
