use hash::H256;

#[derive(Debug)]
pub struct SideChainOrigin {
	/// newest ancestor block number
	pub ancestor: u32,
	/// side chain block hashes. Ordered from oldest to newest
	pub route: Vec<H256>,
	/// new block number
	pub block_number: u32,
}

#[derive(Debug)]
pub enum BlockOrigin {
	KnownBlock,
	CanonChain {
		block_number: u32,
	},
	SideChain(SideChainOrigin),
	SideChainBecomingCanonChain(SideChainOrigin),
}
