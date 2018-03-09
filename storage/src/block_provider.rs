use hash::H256;
use bytes::Bytes;
use chain::{BlockHeader, Transaction, Block, IndexedBlock, IndexedBlockHeader, IndexedTransaction};
use {BlockRef};

pub trait BlockHeaderProvider {
	/// resolves header bytes by block reference (number/hash)
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header(&self, block_ref: BlockRef) -> Option<BlockHeader>;
}

pub trait BlockProvider: BlockHeaderProvider {

	/// resolves number by block hash
	fn block_number(&self, hash: &H256) -> Option<u32>;

	/// resolves hash by block number
	fn block_hash(&self, number: u32) -> Option<H256>;

	/// resolves deserialized block body by block reference (number/hash)
	fn block(&self, block_ref: BlockRef) -> Option<Block>;

	/// returns true if store contains given block
	fn contains_block(&self, block_ref: BlockRef) -> bool {
		self.block_header_bytes(block_ref).is_some()
	}

	/// resolves list of block transactions by block reference (number/hash)
	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256>;

	/// returns all transactions in the block by block reference (number/hash)
	fn block_transactions(&self, block_ref: BlockRef) -> Vec<Transaction>;
}

pub trait IndexedBlockProvider: BlockProvider {
	fn indexed_block_header(&self, block_ref: BlockRef) -> Option<IndexedBlockHeader>;

	fn indexed_block(&self, block_ref: BlockRef) -> Option<IndexedBlock>;

	fn indexed_block_transactions(&self, block_ref: BlockRef) -> Vec<IndexedTransaction>;
}
