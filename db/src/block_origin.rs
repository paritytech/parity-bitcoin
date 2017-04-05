use std::fmt;
use hash::H256;

#[derive(Clone)]
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

impl fmt::Debug for SideChainOrigin {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.debug_struct("SideChainOrigin")
			.field("ancestor", &self.ancestor)
			.field("canonized_route", &self.canonized_route.iter().map(|h| h.reversed()).collect::<Vec<_>>())
			.field("decanonized_route", &self.decanonized_route.iter().map(|h| h.reversed()).collect::<Vec<_>>())
			.field("block_number", &self.block_number)
			.finish()
	}
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
