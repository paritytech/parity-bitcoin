use std::collections::HashMap;
use chain::IndexedBlockHeader;
use storage::{BlockRef, BlockHeaderProvider};
use primitives::bytes::Bytes;
use primitives::hash::H256;

/// Block headers provider from `headers` message
pub struct MessageBlockHeadersProvider<'a> {
	/// Synchronization chain headers provider
	chain_provider: &'a dyn BlockHeaderProvider,
	/// headers offset
	first_header_number: u32,
	/// headers by hash
	headers: HashMap<H256, IndexedBlockHeader>,
	/// headers by order
	headers_order: Vec<H256>,
}

impl<'a> MessageBlockHeadersProvider<'a> {
	pub fn new(chain_provider: &'a dyn BlockHeaderProvider, best_block_header_height: u32) -> Self {
		MessageBlockHeadersProvider {
			chain_provider: chain_provider,
			first_header_number: best_block_header_height + 1,
			headers: HashMap::new(),
			headers_order: Vec::new(),
		}
	}

	pub fn append_header(&mut self, hash: H256, header: IndexedBlockHeader) {
		self.headers.insert(hash.clone(), header);
		self.headers_order.push(hash);
	}
}

impl<'a> BlockHeaderProvider for MessageBlockHeadersProvider<'a> {
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
		use ser::serialize;
		self.block_header(block_ref).map(|h| serialize(&h.raw))
	}

	fn block_header(&self, block_ref: BlockRef) -> Option<IndexedBlockHeader> {
		self.chain_provider.block_header(block_ref.clone())
			.or_else(move || match block_ref {
				BlockRef::Hash(h) => self.headers.get(&h).cloned(),
				BlockRef::Number(n) => if n >= self.first_header_number && n - self.first_header_number < self.headers_order.len() as u32 {
					let header_hash = &self.headers_order[(n - self.first_header_number) as usize];
					Some(self.headers[header_hash].clone())
				} else {
					None
				},
			})
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use storage::{AsSubstore, BlockHeaderProvider, BlockRef};
	use db::BlockChainDatabase;
	use primitives::hash::H256;
	use super::MessageBlockHeadersProvider;

	#[test]
	fn test_message_block_headers_provider() {
		let storage = BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]);
		let storage_provider = storage.as_block_header_provider();
		let mut headers_provider = MessageBlockHeadersProvider::new(storage_provider, 0);

		assert_eq!(headers_provider.block_header(BlockRef::Hash(test_data::genesis().hash())), Some(test_data::genesis().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Number(0)), Some(test_data::genesis().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Hash(H256::from(1))), None);
		assert_eq!(headers_provider.block_header(BlockRef::Number(1)), None);

		headers_provider.append_header(test_data::block_h1().hash(), test_data::block_h1().block_header.into());

		assert_eq!(headers_provider.block_header(BlockRef::Hash(test_data::genesis().hash())), Some(test_data::genesis().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Number(0)), Some(test_data::genesis().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Hash(test_data::block_h1().hash())), Some(test_data::block_h1().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Number(1)), Some(test_data::block_h1().block_header.into()));
		assert_eq!(headers_provider.block_header(BlockRef::Hash(H256::from(1))), None);
		assert_eq!(headers_provider.block_header(BlockRef::Number(2)), None);
	}
}
