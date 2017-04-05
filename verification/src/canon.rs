use std::ops;
use primitives::hash::H256;
use chain::{IndexedBlock, IndexedTransaction, IndexedBlockHeader};

/// Blocks whose parents are known to be in the chain
#[derive(Clone, Copy)]
pub struct CanonBlock<'a> {
	block: &'a IndexedBlock,
}

impl<'a> CanonBlock<'a> {
	pub fn new(block: &'a IndexedBlock) -> Self {
		CanonBlock {
			block: block,
		}
	}

	pub fn hash<'b>(&'b self) -> &'a H256 where 'a: 'b {
		&self.block.header.hash
	}

	pub fn raw<'b>(&'b self) -> &'a IndexedBlock where 'a: 'b {
		self.block
	}

	pub fn header<'b>(&'b self) -> CanonHeader<'a> where 'a: 'b {
		CanonHeader::new(&self.block.header)
	}

	pub fn transactions<'b>(&'b self) -> Vec<CanonTransaction<'a>> where 'a: 'b {
		self.block.transactions.iter().map(CanonTransaction::new).collect()
	}
}

impl<'a> ops::Deref for CanonBlock<'a> {
	type Target = IndexedBlock;

	fn deref(&self) -> &Self::Target {
		self.block
	}
}

#[derive(Clone, Copy)]
pub struct CanonHeader<'a> {
	header: &'a IndexedBlockHeader,
}

impl<'a> CanonHeader<'a> {
	pub fn new(header: &'a IndexedBlockHeader) -> Self {
		CanonHeader {
			header: header,
		}
	}
}

impl<'a> ops::Deref for CanonHeader<'a> {
	type Target = IndexedBlockHeader;

	fn deref(&self) -> &Self::Target {
		self.header
	}
}

#[derive(Clone, Copy)]
pub struct CanonTransaction<'a> {
	transaction: &'a IndexedTransaction,
}

impl<'a> CanonTransaction<'a> {
	pub fn new(transaction: &'a IndexedTransaction) -> Self {
		CanonTransaction {
			transaction: transaction,
		}
	}
}

impl<'a> ops::Deref for CanonTransaction<'a> {
	type Target = IndexedTransaction;

	fn deref(&self) -> &Self::Target {
		self.transaction
	}
}
