use chain::BlockHeader;
use {BlockRef, BlockHeaderProvider};

pub struct BlockAncestors<'a> {
	block: BlockRef,
	headers: &'a BlockHeaderProvider,
}

impl<'a> BlockAncestors<'a> {
	pub fn new(block: BlockRef, headers: &'a BlockHeaderProvider) -> Self {
		BlockAncestors {
			block: block,
			headers: headers,
		}
	}
}

impl<'a> Iterator for BlockAncestors<'a> {
	type Item = BlockHeader;

	fn next(&mut self) -> Option<Self::Item> {
		let result = self.headers.block_header(self.block.clone());
		if let Some(ref header) = result {
			self.block = BlockRef::Hash(header.previous_header_hash.clone());
		}
		result
	}
}
