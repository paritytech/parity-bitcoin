use primitives::hash::H256;
use chain::BlockHeader;

#[derive(Debug, Clone)]
pub struct IndexedBlockHeader {
	pub hash: H256,
	pub raw: BlockHeader,
}

impl From<BlockHeader> for IndexedBlockHeader {
	fn from(header: BlockHeader) -> Self {
		IndexedBlockHeader {
			hash: header.hash(),
			raw: header,
		}
	}
}

impl IndexedBlockHeader {
	pub fn new(hash: H256, header: BlockHeader) -> Self {
		IndexedBlockHeader {
			hash: hash,
			raw: header,
		}
	}
}
