use primitives::hash::H256;

#[derive(Debug, Clone)]
pub struct BestBlock {
	pub height: u64,
	pub hash: H256,
}
