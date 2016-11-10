//! Test storage

use super::{BlockRef, Store, Error, BestBlock, BlockLocation};
use chain::{self, Block, RepresentH256};
use primitives::hash::H256;
use serialization;
use chain::bytes::Bytes;
use std::mem::replace;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use parking_lot::RwLock;
use transaction_meta::TransactionMeta;

#[derive(Default)]
pub struct TestStorage {
	data: RwLock<TestData>,
}

#[derive(Default)]
struct TestData {
	best_block: Option<BestBlock>,
	blocks: HashMap<H256, chain::Block>,
	heights: HashMap<u32, H256>,
	hashes: HashMap<H256, u32>,
}

impl TestStorage {
	fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
		match block_ref {
			BlockRef::Number(n) => self.block_hash(n),
			BlockRef::Hash(h) => Some(h),
		}
	}

	pub fn with_blocks(blocks: &[chain::Block]) -> Self {
		let blocks_len = blocks.len();
		let storage = TestStorage::default();
		{
			let mut data = storage.data.write();
			if blocks_len != 0 {
				data.best_block = Some(BestBlock {
					number: blocks_len as u32 - 1,
					hash: blocks[blocks_len - 1].hash(),
				});
				for (idx, block) in blocks.iter().enumerate() {
					let hash = block.hash();
					data.blocks.insert(hash.clone(), block.clone());
					data.heights.insert(idx as u32, hash.clone());
					data.hashes.insert(hash, idx as u32);
				}
			}
		}
		storage
	}

	pub fn with_genesis_block() -> Self {
		let genesis_block: Block = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into();
		TestStorage::with_blocks(&vec![genesis_block])
	}
}

impl Store for TestStorage {
	fn best_block(&self) -> Option<BestBlock> {
		self.data.read().best_block.clone()
	}

	fn block_number(&self, hash: &H256) -> Option<u32> {
		let data = self.data.read();
		data.hashes.get(hash).cloned()
	}

	fn block_hash(&self, number: u32) -> Option<H256> {
		let data = self.data.read();
		data.heights.get(&number).cloned()
	}

	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		let data = self.data.read();
		self.resolve_hash(block_ref)
			.and_then(|ref h| data.blocks.get(h))
			.map(|ref block| serialization::serialize(block.header()))
	}

	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
		let data = self.data.read();
		self.resolve_hash(block_ref)
			.and_then(|ref h| data.blocks.get(h))
			.map(|ref block| block.transactions().iter().map(|tx| tx.hash()).collect())
			.unwrap_or(Vec::new())
	}

	fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
		self.transaction(hash).map(|tx| serialization::serialize(&tx))
	}

	fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
		let data = self.data.read();
		data.blocks.iter().flat_map(|(_, b)| b.transactions())
			.find(|ref tx| tx.hash() == *hash)
			.cloned()
	}

	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
		self.block(block_ref)
			.map(|b| b.transactions().iter().cloned().collect())
			.unwrap_or(Vec::new())
	}

	fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
		let data = self.data.read();
		self.resolve_hash(block_ref)
			.and_then(|ref h| data.blocks.get(h))
			.cloned()
	}

	fn insert_block(&self, block: &chain::Block) -> Result<(), Error> {
		let hash = block.hash();
		let mut data = self.data.write();
		match data.blocks.entry(hash.clone()) {
			Entry::Occupied(mut entry) => {
				replace(entry.get_mut(), block.clone());
				return Ok(());
			},
			Entry::Vacant(entry) => {
				entry.insert(block.clone());
			},
		}
		match data.best_block {
			Some(BestBlock { number: best_block_number, hash: _ }) => {
				data.best_block = Some(BestBlock {
					number: best_block_number + 1,
					hash: hash.clone(),
				});
				data.heights.insert(best_block_number + 1, hash.clone());
				data.hashes.insert(hash, best_block_number + 1);
			},
			None => {
				data.best_block = Some(BestBlock {
					number: 0,
					hash: hash.clone(),
				});
				data.heights.insert(0, hash.clone());
				data.hashes.insert(hash, 0);
			},
		}

		Ok(())
	}

	// just spawns new meta so far, use real store for proper tests
	fn transaction_meta(&self, hash: &H256) -> Option<TransactionMeta> {
		self.transaction(hash).map(|tx| TransactionMeta::new(0, tx.outputs.len()))
	}

	// supports only main chain in test storage
	fn accepted_location(&self, header: &chain::BlockHeader) -> Option<BlockLocation> {
		if self.best_block().is_none() { return Some(BlockLocation::Main(0)); }

		let best = self.best_block().unwrap();
		if best.hash == header.previous_header_hash { return Some(BlockLocation::Main(best.number + 1)); }

		None
	}

}

