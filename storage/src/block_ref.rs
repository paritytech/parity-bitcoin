use hash::H256;

#[derive(Debug, Clone)]
pub enum BlockRef {
	Number(u32),
	Hash(H256),
}

impl From<u32> for BlockRef {
	fn from(u: u32) -> Self {
		BlockRef::Number(u)
	}
}

impl From<H256> for BlockRef {
	fn from(hash: H256) -> Self {
		BlockRef::Hash(hash)
	}
}
