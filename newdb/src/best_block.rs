use std::fmt;
use hash::H256;

/// Best block information
#[derive(Clone, PartialEq, Default)]
pub struct BestBlock {
	/// Height/number of the best block (genesis block has zero height)
	pub number: u32,
	/// Hash of the best block
	pub hash: H256,
}

impl fmt::Debug for BestBlock {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("BestBlock")
			.field("number", &self.number)
			.field("hash", &self.hash.reversed())
			.finish()
	}
}
