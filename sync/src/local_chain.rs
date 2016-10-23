use std::collections::HashSet;
use primitives::hash::H256;
use best_block::BestBlock;

pub struct LocalChain {
	blocks_order: Vec<H256>,
	blocks_map: HashSet<H256>,
}

impl LocalChain {
	pub fn new() -> LocalChain {
		let mut chain = LocalChain {
			blocks_order: Vec::new(),
			blocks_map: HashSet::new(),
		};

		let genesis_hash: H256 = "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into();
		chain.blocks_order.push(genesis_hash.clone());
		chain.blocks_map.insert(genesis_hash);
		chain
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
}