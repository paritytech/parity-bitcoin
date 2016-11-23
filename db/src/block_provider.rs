use super::BlockRef;
use primitives::hash::H256;
use primitives::bytes::Bytes;
use chain;

pub trait BlockProvider {

		/// resolves number by block hash
	fn block_number(&self, hash: &H256) -> Option<u32>;

	/// resolves hash by block number
	fn block_hash(&self, number: u32) -> Option<H256>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes>;

	/// resolves header bytes by block reference (number/hash)
	fn block_header(&self, block_ref: BlockRef) -> Option<chain::BlockHeader>;

	/// resolves deserialized block body by block reference (number/hash)
	fn block(&self, block_ref: BlockRef) -> Option<chain::Block>;

	/// returns true if store contains given block
	fn contains_block(&self, block_ref: BlockRef) -> bool {
		self.block_header_bytes(block_ref).is_some()
	}

	/// resolves list of block transactions by block reference (number/hash)
	fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256>;

	/// returns all transactions in the block by block reference (number/hash)
	fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction>;

}
