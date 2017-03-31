use primitives::hash::H256;
use chain::{self, IndexedBlock};
use error::Error;
use super::BlockLocation;

#[derive(Debug, PartialEq, Clone)]
pub struct Reorganization {
	pub height: u32,
	canonized: Vec<H256>,
	decanonized: Vec<H256>,
}

impl Reorganization {
	pub fn new(height: u32) -> Reorganization {
		Reorganization { height: height, canonized: Vec::new(), decanonized: Vec::new() }
	}

	pub fn push_canonized(&mut self, hash: &H256) {
		self.canonized.push(hash.clone());
	}

	pub fn push_decanonized(&mut self, hash: &H256) {
		self.decanonized.push(hash.clone());
	}

	pub fn pop_canonized(&mut self) -> Option<H256> {
		self.canonized.pop()
	}

	pub fn pop_decanonized(&mut self) -> Option<H256> {
		self.decanonized.pop()
	}
}

#[derive(Debug, PartialEq, Clone)]
pub enum BlockInsertedChain {
	Disconnected,
	Main,
	Side,
	Reorganized(Reorganization),
}

pub trait BlockStapler {
	/// return the location of this block once if it ever gets inserted
	fn accepted_location(&self, header: &chain::BlockHeader) -> Option<BlockLocation>;

	/// insert block in the storage
	fn insert_block(&self, block: &chain::Block) -> Result<BlockInsertedChain, Error>;

	/// insert pre-processed block in the storage
	fn insert_indexed_block(&self, block: &IndexedBlock) -> Result<BlockInsertedChain, Error>;

	/// flushes the underlined store (if applicable)
	fn flush(&self);

}
