use chain::{IndexedBlock, IndexedBlockHeader};
use {Error, BlockOrigin, Store, SideChainOrigin};

pub trait ForkChain {
	/// Returns forks underlaying store.
	fn store(&self) -> &Store;

	/// Flush fork changes to canon chain.
	/// Should not be used directly from outside of `BlockChain`.
	fn flush(&self) -> Result<(), Error>;
}

pub trait BlockChain {
	/// Inserts new block into blockchain
	fn insert(&self, block: &IndexedBlock) -> Result<(), Error>;

	/// Checks block origin
	fn block_origin(&self, header: &IndexedBlockHeader) -> Result<BlockOrigin, Error>;

	/// Forks current blockchain.
	/// Lifetime guarantees fork relationship with canon chain.
	fn fork<'a>(&'a self, side_chain: SideChainOrigin) -> Result<Box<ForkChain + 'a>, Error>;

	/// Switches blockchain to given fork.
	/// Lifetime guarantees that fork comes from this canon chain.
	fn switch_to_fork<'a>(&'a self, fork: Box<ForkChain + 'a>) -> Result<(), Error>;
}
