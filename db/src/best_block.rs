use primitives::hash::H256;

/// Best block information
#[derive(Debug, Clone)]
pub struct BestBlock {
	/// Height/number of the best block (genesis block has zero height)
	pub number: u32,
	/// Hash of the best block
	pub hash: H256,
}
