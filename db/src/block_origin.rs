use hash::H256;

#[derive(Debug, Clone)]
pub struct SideChainOrigin {
	/// newest ancestor block number
	pub ancestor: u32,
	/// side chain block hashes. Ordered from oldest to newest
	pub canonized_route: Vec<H256>,
	/// canon chain block hahses. Ordered from oldest to newest
	pub decanonized_route: Vec<H256>,
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
