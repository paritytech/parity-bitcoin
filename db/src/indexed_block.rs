use chain;
use primitives::hash::H256;
use serialization::Serializable;
use PreviousTransactionOutputProvider;

#[derive(Debug)]
pub struct IndexedBlock {
	header: chain::BlockHeader,
	header_hash: H256,
	transactions: Vec<chain::Transaction>,
	// guaranteed to be the same length as transactions
	transaction_hashes: Vec<H256>,
}

impl PreviousTransactionOutputProvider for IndexedBlock {
	fn previous_transaction_output(&self, prevout: &chain::OutPoint) -> Option<chain::TransactionOutput> {
		self.transaction(&prevout.hash)
			.and_then(|tx| tx.outputs.get(prevout.index as usize))
			.cloned()
	}
}

impl From<chain::Block> for IndexedBlock {
	fn from(block: chain::Block) -> Self {
		let chain::Block { block_header, transactions } = block;
		let header_hash = block_header.hash();
		let hashes = transactions.iter().map(chain::Transaction::hash).collect();

		IndexedBlock {
			header: block_header,
			header_hash: header_hash,
			transactions: transactions,
			transaction_hashes: hashes,
		}
	}
}

impl IndexedBlock {
	pub fn new(header: chain::BlockHeader, transaction_index: Vec<(H256, chain::Transaction)>) -> Self {
		let mut block = IndexedBlock {
			header_hash: header.hash(),
			header: header,
			transactions: Vec::with_capacity(transaction_index.len()),
			transaction_hashes: Vec::with_capacity(transaction_index.len()),
		};

		for (h256, tx) in transaction_index {
			block.transactions.push(tx);
			block.transaction_hashes.push(h256);
		}

		block
	}

	pub fn transaction(&self, hash: &H256) -> Option<&chain::Transaction> {
		self.transaction_hashes.iter()
			.position(|x| x == hash)
			.map(|position| &self.transactions[position])
	}

	pub fn transactions(&self) -> IndexedTransactions {
		IndexedTransactions {
			position: 0,
			block: self,
		}
	}

	pub fn transaction_hashes(&self) -> &[H256] {
		&self.transaction_hashes
	}

	pub fn header(&self) -> &chain::BlockHeader {
		&self.header
	}

	pub fn hash(&self) -> &H256 {
		&self.header_hash
	}

	pub fn transaction_count(&self) -> usize {
		self.transaction_hashes.len()
	}

	pub fn to_block(&self) -> chain::Block {
		chain::Block::new(
			self.header.clone(),
			self.transactions.clone(),
		)
	}

	pub fn size(&self) -> usize {
		// todo: optimize
		self.to_block().serialized_size()
	}

	pub fn merkle_root(&self) -> H256 {
		chain::merkle_root(&self.transaction_hashes)
	}

	pub fn is_final(&self, height: u32) -> bool {
		self.transactions.iter().all(|t| t.is_final(height, self.header.time))
	}

	pub fn transaction_at(&self, index: usize) -> (&H256, &chain::Transaction) {
		(&self.transaction_hashes[index], &self.transactions[index])
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
			let result = Some((&self.block.transaction_hashes[self.position], &self.block.transactions[self.position]));
			self.position += 1;
			result
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

	#[test]
	fn iter() {
		let block = test_data::block_builder()
			.header().build()
			.transaction().coinbase().output().value(3).build().build()
			.transaction().coinbase().output().value(5).build().build()
			.build();
		let indexed_block: IndexedBlock = block.clone().into();

		assert_eq!(*indexed_block.transactions().nth(1).unwrap().0, block.transactions()[1].hash());
	}
}
