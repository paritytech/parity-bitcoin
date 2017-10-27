use chain::BlockHeader;
use {BlockRef, BlockHeaderProvider};

pub struct BlockAncestors<'a> {
	block: Option<BlockRef>,
	headers: &'a BlockHeaderProvider,
}

impl<'a> BlockAncestors<'a> {
	pub fn new(block: BlockRef, headers: &'a BlockHeaderProvider) -> Self {
		BlockAncestors {
			block: Some(block),
			headers: headers,
		}
	}
}

impl<'a> Iterator for BlockAncestors<'a> {
	type Item = BlockHeader;

	fn next(&mut self) -> Option<Self::Item> {
		let result = self.block.take().and_then(|block| self.headers.block_header(block));
		self.block = match result {
			Some(ref header) => Some(BlockRef::Hash(header.previous_header_hash.clone())),
			None => None,
		};
		result
	}
}
