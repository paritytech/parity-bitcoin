use std::sync::Arc;
use devtools::RandomTempPath;
use db::{Storage, BlockStapler, Store};
use chain::IndexedBlock;
use verification::{BackwardsCompatibleChainVerifier as ChainVerifier, Verify};
use network::Magic;
use test_data;
use byteorder::{LittleEndian, ByteOrder};
use primitives::Compact;

use super::Benchmark;

// 1. write BLOCKS_INITIAL blocks with 1 transaction each
// 2. verify <BLOCKS> blocks that has <TRANSACTIONS> transaction each with <OUTPUTS> output each,
//    spending outputs from last <BLOCKS*TRANSACTIONS*OUTPUTS> blocks
pub fn main(benchmark: &mut Benchmark) {
	// params
	const BLOCKS_INITIAL: usize = 200200;
	const BLOCKS: usize = 10;
	const TRANSACTIONS: usize = 2000;
	const OUTPUTS: usize = 10;

	benchmark.samples(BLOCKS);

	assert!(BLOCKS_INITIAL - 100 > BLOCKS * OUTPUTS * TRANSACTIONS,
		"There will be not enough initial blocks to continue this bench");

	// test setup
	let path = RandomTempPath::create_dir();

	let genesis = test_data::genesis();

	let mut rolling_hash = genesis.hash();
	let mut blocks: Vec<IndexedBlock> = Vec::new();

	for x in 0..BLOCKS_INITIAL {
		let mut coinbase_nonce = [0u8;8];
		LittleEndian::write_u64(&mut coinbase_nonce[..], x as u64);
		let next_block = test_data::block_builder()
			.transaction()
				.input()
					.coinbase()
					.signature_bytes(coinbase_nonce.to_vec().into())
					.build()
				.output().value(5000000000).build()
				.build()
			.merkled_header()
				.parent(rolling_hash.clone())
				.nonce(x as u32)
				.build()
			.build();
		rolling_hash = next_block.hash();
		blocks.push(next_block.into());
	}

	{
		let store = Arc::new(Storage::new(path.as_path()).unwrap());
		store.insert_block(&genesis).unwrap();
		for block in blocks.iter() { store.insert_indexed_block(block).unwrap(); }
	}

	let mut verification_blocks: Vec<IndexedBlock> = Vec::new();
	for b in 0..BLOCKS {
		let mut coinbase_nonce = [0u8;8];
		LittleEndian::write_u64(&mut coinbase_nonce[..], (b + BLOCKS_INITIAL) as u64);
		let mut builder = test_data::block_builder()
			.transaction()
				.input().coinbase().signature_bytes(coinbase_nonce.to_vec().into()).build()
				.output().value(5000000000).build()
				.build();

		for t in 0..TRANSACTIONS {
			let mut tx_builder = builder.transaction();

			for o in 0..OUTPUTS {
				let parent_hash = blocks[(b*TRANSACTIONS*OUTPUTS + t * OUTPUTS + o)].transactions[0].hash.clone();

				tx_builder = tx_builder
					.input()
						.hash(parent_hash)
						.index(0)
						.build()
			}

			builder = tx_builder.output().value(0).build().build()
		}

		verification_blocks.push(
			builder
				.merkled_header()
					.parent(rolling_hash.clone())
					.build()
				.build()
			.into());
	}


	let store = Arc::new(Storage::new(path.as_path()).unwrap());
	assert_eq!(store.best_block().unwrap().hash, rolling_hash);

	let chain_verifier = ChainVerifier::new(store.clone(), Magic::Mainnet).pow_skip();

	// bench
	benchmark.start();
	for block in verification_blocks.iter() {
		chain_verifier.verify(block).unwrap();
	 }
	benchmark.stop();
}
