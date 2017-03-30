
#[derive(Debug)]
pub enum BlockOrigin {
	KnownBlock,
	CanonChain,
	SideChain,
	SideChainBecomingCanonChain,
}
