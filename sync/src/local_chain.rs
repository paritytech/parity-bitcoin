use std::collections::HashMap;
use std::collections::hash_map::Entry;
use chain::{Block, RepresentH256};
use primitives::hash::H256;
use best_block::BestBlock;

#[derive(Debug)]
pub struct Info {
	pub chain_length: usize,
	pub orphan_count: usize,
}

// TODO: this is temp storage (to use during test stage)
//     it must be replaced with db + verification queue + mempools (transaction, block, ...)
pub struct LocalChain {
	blocks_order: Vec<H256>,
	blocks_map: HashMap<H256, Block>,
	orphan_blocks: HashMap<H256, Block>,
}

impl LocalChain {
	pub fn new() -> LocalChain {
		let mut chain = LocalChain {
			blocks_order: Vec::new(),
			blocks_map: HashMap::new(),
			orphan_blocks: HashMap::new(),
		};

		// TODO: move this to config
		let genesis_block: Block = "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into();
		let genesis_block_hash = genesis_block.hash();

		chain.blocks_order.push(genesis_block_hash.clone());
		chain.blocks_map.insert(genesis_block_hash, genesis_block);
		chain
	}

	pub fn info(&self) -> Info {
		Info {
			chain_length: self.blocks_order.len(),
			orphan_count: self.orphan_blocks.len(),
		}
	}

	pub fn best_block(&self) -> BestBlock {
		let height = self.blocks_order.len() - 1;
		let ref block = self.blocks_order[height];
		BestBlock {
			height: height as u64,
			hash: block.clone(),
		}
	}

	pub fn block_locator_hashes(&self) -> Vec<H256> {
		let mut index = self.blocks_order.len() - 1;
		let mut hashes: Vec<H256> = Vec::new();
		let mut step = 1;
		loop {
			let block_hash = self.blocks_order[index].clone();
			hashes.push(block_hash);

			if hashes.len() >= 10 {
				step <<= 1;
			}
			if index < step {
				break;
			}
			index -= step;
		}

		hashes
	}

	pub fn is_known_block(&self, hash: &H256) -> bool {
		self.blocks_map.contains_key(hash)
	}

	pub fn insert_block(&mut self, block: &Block) {
		// check if already known block
		let block_header_hash = block.block_header.hash();
		if self.blocks_map.contains_key(&block_header_hash) {
			return;
		}

		// check if parent block is in the storage
		// if there is no parent block for this block, remember as orphaned
		if !self.blocks_map.contains_key(&block.block_header.previous_header_hash) {
			self.orphan_blocks.insert(block.block_header.previous_header_hash.clone(), block.clone());
			return;
		}

		// insert block
		for i in 0..self.blocks_order.len() {
			if self.blocks_order[i] == block.block_header.previous_header_hash {
				self.blocks_order.insert(i + 1, block_header_hash.clone());
				self.blocks_map.insert(block_header_hash.clone(), block.clone());

				// TODO: forks
				// check if any orphan blocks now can be moved to the blockchain
				let mut position = i + 1;
				let mut block_header_hash = block_header_hash;
				while let Entry::Occupied(orphan_block_entry) = self.orphan_blocks.entry(block_header_hash.clone()) {
					// remove from orphans
					let (_, orphan_block) = orphan_block_entry.remove_entry();
					let orphan_block_hash = orphan_block.hash();

					// insert to blockchain
					self.blocks_map.insert(block_header_hash.clone(), orphan_block);
					block_header_hash = orphan_block_hash;

					// insert to ordering
					self.blocks_order.insert(position + 1, block_header_hash.clone());
					position += 1;
				}
				return;
			}
		}
	}
}
