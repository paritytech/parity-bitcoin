use chain;
use primitives::hash::H256;

pub struct IndexedBlock {
	header: chain::BlockHeader,
	header_hash: H256,
	transactions: Vec<chain::Transaction>,
	// guaranteed to be the same length as transactions
	transaction_hashes: Vec<H256>,
}

impl From<chain::Block> for IndexedBlock {
	fn from(block: chain::Block) -> Self {
		let (org_header, org_txs) = block.drain();
		let mut hashes = Vec::with_capacity(org_txs.len());
		for tx in org_txs.iter() {
			hashes.push(tx.hash())
		}
		let header_hash = org_header.hash();

		IndexedBlock {
			header: org_header,
			header_hash: header_hash,
			transactions: org_txs,
			transaction_hashes: hashes,
		}
	}
}

impl IndexedBlock {
	pub fn transactions(&self) -> IndexedTransactions {
		IndexedTransactions {
			position: 0,
			block: self,
		}
	}

	pub fn header(&self) -> &chain::BlockHeader {
		&self.header
	}

	pub fn hash(&self) -> &H256 {
		&self.header_hash
	}
}

pub struct IndexedTransactions<'a> {
	position: usize,
	block: &'a IndexedBlock,
}

impl<'a> Iterator for IndexedTransactions<'a> {
	type Item = (&'a H256, &'a chain::Transaction);

	fn next(&mut self) -> Option<(&'a H256, &'a chain::Transaction)> {
		if self.position >= self.block.transactions.len() {
			None
		}
		else {
			Some((&self.block.transaction_hashes[self.position], &self.block.transactions[self.position]))
		}
	}
}

#[cfg(test)]
mod tests {
	use test_data;
	use super::IndexedBlock;

	#[test]
	fn index() {
		let block = test_data::block_h1();
		let indexed_block: IndexedBlock = block.clone().into();

		assert_eq!(*indexed_block.transactions().nth(0).unwrap().0, block.transactions()[0].hash());
	}
}
