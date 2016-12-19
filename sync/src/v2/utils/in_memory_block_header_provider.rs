use std::collections::HashMap;
use chain::{BlockHeader, IndexedBlockHeader};
use db::{BlockRef, BlockHeaderProvider};
use primitives::bytes::Bytes;
use primitives::hash::H256;
use ser::serialize;
use super::{BlockHeight, BlockHeaderChain};

/// Block headers provider from `headers` message
pub struct InMemoryBlockHeaderProvider<'a> {
	/// Block headers chain
	headers_chain: &'a BlockHeaderChain,
	/// Previous header (from headers_chain)
	previous_header_height: Option<BlockHeight>,
	/// Header-by-hash
	by_hash: HashMap<H256, IndexedBlockHeader>,
	/// headers by order
	by_height: Vec<H256>,
}

impl<'a> InMemoryBlockHeaderProvider<'a> {
	/// Creates new provider for given headers chain and previous header reference
	pub fn new(headers_chain: &'a BlockHeaderChain, previous_header_height: Option<BlockHeight>) -> Self {
		InMemoryBlockHeaderProvider {
			headers_chain: headers_chain,
			previous_header_height: previous_header_height,
			by_hash: HashMap::new(),
			by_height: Vec::new(),
		}
	}

	/// Appends new header to the end of in-memory chain
	pub fn append_header(&mut self, header: IndexedBlockHeader) {
		self.by_height.push(header.hash.clone());
		self.by_hash.insert(header.hash.clone(), header);
	}
}

impl<'a> BlockHeaderProvider for InMemoryBlockHeaderProvider<'a> {
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		self.block_header(block_ref).map(|h| serialize(&h))
	}

	fn block_header(&self, block_ref: BlockRef) -> Option<BlockHeader> {
		match block_ref {
			BlockRef::Hash(h) => self.headers_chain.by_hash(&h)
				.or_else(|| self.by_hash.get(&h).cloned()),
			BlockRef::Number(n) => {
				if let Some(previous_header_height) = self.previous_header_height {
					if n > previous_header_height {
						let n = (n - previous_header_height - 1) as usize;
						if n < self.by_height.len() {
							return Some(self.by_hash[&self.by_height[n]].raw.clone())
						} else {
							return None
						}
					}
				}

				self.headers_chain.at(n)
			}
		}
		.map(|header| header.raw)
	}
}

#[cfg(test)]
mod tests {
	use db::BlockHeaderProvider;
	use test_data;
	use super::super::BlockHeaderChain;
	use super::super::BlockHeight;
	use super::InMemoryBlockHeaderProvider;

	#[test]
	fn in_memory_block_header_provider_updates() {
		let chain = BlockHeaderChain::new(test_data::genesis().hash());
		let mut provider = InMemoryBlockHeaderProvider::new(&chain, Some(0));
		assert_eq!(provider.block_header(0.into()), Some(test_data::genesis().block_header.into()));
		assert_eq!(provider.block_header(1.into()), None);
		assert_eq!(provider.block_header(test_data::genesis().hash().into()), Some(test_data::genesis().block_header.into()));
		assert_eq!(provider.block_header(test_data::block_h1().hash().into()), None);
		provider.append_header(test_data::block_h1().block_header.into());
		assert_eq!(provider.block_header(0.into()), Some(test_data::genesis().block_header.into()));
		assert_eq!(provider.block_header(1.into()), Some(test_data::block_h1().block_header.into()));
		assert_eq!(provider.block_header(test_data::genesis().hash().into()), Some(test_data::genesis().block_header.into()));
		assert_eq!(provider.block_header(test_data::block_h1().hash().into()), Some(test_data::block_h1().block_header.into()));
	}
}