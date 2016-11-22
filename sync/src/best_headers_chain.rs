use std::collections::HashMap;
use chain::BlockHeader;
use primitives::hash::H256;
use hash_queue::{HashQueue, HashPosition};

/// Best headers chain information
#[derive(Debug)]
pub struct Information {
	/// Number of headers in best chain
	pub best: u32,
	/// Total number of headers
	pub total: u32,
}

// TODO: currently it supports first chain only (so whatever headers sequence came first, it is best)
/// Builds the block-header-chain of in-memory blocks, for which only headers are currently known
#[derive(Debug)]
pub struct BestHeadersChain {
	/// Best hash in storage
	storage_best_hash: H256,
	/// Headers by hash
	headers: HashMap<H256, BlockHeader>,
	/// Best chain
	best: HashQueue,
}

impl BestHeadersChain {
	pub fn new(storage_best_hash: H256) -> Self {
		BestHeadersChain {
			storage_best_hash: storage_best_hash,
			headers: HashMap::new(),
			best: HashQueue::new(),
		}
	}

	pub fn information(&self) -> Information {
		Information {
			best: self.best.len(),
			total: self.headers.len() as u32,
		}
	}

	pub fn at(&self, height: u32) -> Option<BlockHeader> {
		self.best.at(height)
			.and_then(|hash| self.headers.get(&hash).cloned())
	}

	pub fn by_hash(&self, hash: &H256) -> Option<BlockHeader> {
		self.headers.get(hash).cloned()
	}

	pub fn height(&self, hash: &H256) -> Option<u32> {
		self.best.position(hash)
	}

	pub fn children(&self, hash: &H256) -> Vec<H256> {
		self.best.position(hash)
			.and_then(|pos| self.best.at(pos + 1))
			.and_then(|child| Some(vec![child]))
			.unwrap_or_default()
	}

	pub fn best_block_hash(&self) -> H256 {
		self.best.back().or_else(|| Some(self.storage_best_hash.clone())).expect("storage_best_hash is always known")
	}

	pub fn insert(&mut self, header: BlockHeader) {
		// append to the best chain
		if self.best_block_hash() == header.previous_header_hash {
			let header_hash = header.hash();
			self.headers.insert(header_hash.clone(), header);
			self.best.push_back(header_hash);
			return;
		}
	}

	pub fn insert_n(&mut self, headers: Vec<BlockHeader>) {
		for header in headers {
			self.insert(header);
		}
	}

	pub fn remove(&mut self, hash: &H256) {
		if self.headers.remove(hash).is_some() {
			match self.best.remove(hash) {
				HashPosition::Front => self.clear(),
				HashPosition::Inside(position) => self.clear_after(position),
				_ => (),
			}
		}
	}

	pub fn remove_n<I: IntoIterator<Item=H256>> (&mut self, hashes: I) {
		for hash in hashes {
			self.remove(&hash);
		}
	}

	pub fn block_inserted_to_storage(&mut self, hash: &H256, storage_best_hash: &H256) {
		if self.best.front().map(|h| &h == hash).unwrap_or(false) {
			self.best.pop_front();
			self.headers.remove(hash);
		}
		self.storage_best_hash = storage_best_hash.clone();
	}

	pub fn clear(&mut self) {
		self.headers.clear();
		self.best.clear();
	}

	fn clear_after(&mut self, position: u32) {
		if position == 0 {
			self.clear()
		} else {
			while self.best.len() > position {
				self.headers.remove(&self.best.pop_back().expect("len() > position; qed"));
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::BestHeadersChain;
	use primitives::hash::H256;
	use test_data;

	#[test]
	fn best_chain_empty() {
		let chain = BestHeadersChain::new(H256::default());
		assert_eq!(chain.at(0), None);
		assert_eq!(chain.by_hash(&H256::from(0)), None);
		assert_eq!(chain.height(&H256::default()), None);
		assert_eq!(chain.children(&H256::default()), vec![]);
		assert_eq!(chain.best_block_hash(), H256::default());
	}

	#[test]
	fn best_chain_insert() {
		let mut chain = BestHeadersChain::new(test_data::genesis().hash());
		let b1 = test_data::block_h1().block_header;
		let b2 = test_data::block_h2().block_header;
		let b181 = test_data::block_h181().block_header;
		let b182 = test_data::block_h182().block_header;
		chain.insert(b1);
		chain.insert(b181.clone());
		assert_eq!(chain.information().best, 1);
		assert_eq!(chain.information().total, 1);
		chain.insert(b2);
		assert_eq!(chain.information().best, 2);
		assert_eq!(chain.information().total, 2);
		chain.clear();
		assert_eq!(chain.information().best, 0);
		assert_eq!(chain.information().total, 0);
		chain.insert(b181.clone());
		assert_eq!(chain.information().best, 0);
		assert_eq!(chain.information().total, 0);
		chain.block_inserted_to_storage(&b181.hash(), &b181.hash());
		assert_eq!(chain.information().best, 0);
		assert_eq!(chain.information().total, 0);
		chain.insert(b182);
		assert_eq!(chain.information().best, 1);
		assert_eq!(chain.information().total, 1);
	}

	#[test]
	fn best_chain_remove() {
		let b0 = test_data::block_builder().header().build().build();
		let b1 = test_data::block_builder().header().parent(b0.hash()).build().build().block_header;
		let b2 = test_data::block_builder().header().parent(b1.hash()).build().build().block_header;
		let b3 = test_data::block_builder().header().parent(b2.hash()).build().build().block_header;
		let b4 = test_data::block_builder().header().parent(b3.hash()).build().build().block_header;
		let mut chain = BestHeadersChain::new(b0.hash());

		chain.insert_n(vec![b1.clone(), b2.clone(), b3.clone(), b4.clone()]);
		assert_eq!(chain.information().best, 4);
		assert_eq!(chain.information().total, 4);
		chain.remove(&b2.hash());
		assert_eq!(chain.information().best, 1);
		assert_eq!(chain.information().total, 1);

		chain.insert_n(vec![b2.clone(), b3.clone(), b4.clone()]);
		assert_eq!(chain.information().best, 4);
		assert_eq!(chain.information().total, 4);
		chain.remove(&H256::default());
		assert_eq!(chain.information().best, 4);
		assert_eq!(chain.information().total, 4);

		chain.remove(&b1.hash());
		assert_eq!(chain.information().best, 0);
		assert_eq!(chain.information().total, 0);
	}

	#[test]
	fn best_chain_insert_to_db_no_reorg() {
		let mut chain = BestHeadersChain::new(test_data::genesis().hash());
		let b1 = test_data::block_h1().block_header;
		chain.insert(b1.clone());
		assert_eq!(chain.at(0), Some(b1.clone()));
		let b2 = test_data::block_h2().block_header;
		chain.insert(b2.clone());
		assert_eq!(chain.at(0), Some(b1.clone()));
		assert_eq!(chain.at(1), Some(b2.clone()));

		chain.block_inserted_to_storage(&b1.hash(), &b1.hash());

		assert_eq!(chain.at(0), Some(b2));
		assert_eq!(chain.at(1), None);

		assert_eq!(chain.information().best, 1);
		assert_eq!(chain.information().total, 1);
	}
}
