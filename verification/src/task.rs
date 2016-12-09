use chain_verifier::ChainVerifier;
use super::TransactionError;
use db::IndexedBlock;

pub struct Task<'a> {
	block: &'a IndexedBlock,
	block_height: u32,
	from: usize,
	to: usize,
	result: Result<(), TransactionCheckError>,
}

type TransactionCheckError = (usize, TransactionError);

impl<'a> Task<'a> {
	pub fn new(block: &'a IndexedBlock, block_height: u32, from: usize, to: usize) -> Self {
		Task {
			block: block,
			block_height: block_height,
			from: from,
			to: to,
			result: Ok(()),
		}
	}

	pub fn progress(&mut self, verifier: &ChainVerifier) {
		for index in self.from..self.to {
			if let Err(e) = verifier.verify_transaction(self.block, self.block_height, self.block.header.raw.time, &self.block.transactions[index].raw, index) {
				self.result = Err((index, e))
			}
		}
		self.result = Ok(());
	}

	pub fn result(self) -> Result<(), TransactionCheckError> {
		self.result
	}
}
