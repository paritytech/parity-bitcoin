use primitives::hash::H256;
use chain::{Block, BlockHeader, OutPoint, TransactionOutput, merkle_root};
use serialization::Serializable;
use indexed_header::IndexedBlockHeader;
use indexed_transaction::IndexedTransaction;
use PreviousTransactionOutputProvider;

#[derive(Debug, Clone)]
pub struct IndexedBlock {
	pub header: IndexedBlockHeader,
	pub transactions: Vec<IndexedTransaction>,
}

impl PreviousTransactionOutputProvider for IndexedBlock {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		let txs: &[_] = &self.transactions;
		txs.previous_transaction_output(prevout)
	}

	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		unimplemented!();
	}
}

impl From<Block> for IndexedBlock {
	fn from(block: Block) -> Self {
		let Block { block_header, transactions } = block;

		IndexedBlock {
			header: block_header.into(),
			transactions: transactions.into_iter().map(Into::into).collect(),
		}
	}
}

impl IndexedBlock {
	pub fn new(header: IndexedBlockHeader, transactions: Vec<IndexedTransaction>) -> Self {
		IndexedBlock {
			header: header,
			transactions: transactions,
		}
	}

	pub fn hash(&self) -> &H256 {
		&self.header.hash
	}

	pub fn header(&self) -> &BlockHeader {
		&self.header.raw
	}

	pub fn to_raw_block(self) -> Block {
		Block::new(self.header.raw, self.transactions.into_iter().map(|tx| tx.raw).collect())
	}

	pub fn size(&self) -> usize {
		let txs_size = self.transactions.iter().map(|tx| tx.raw.serialized_size()).sum::<usize>();
		self.header.raw.serialized_size() + txs_size
	}

	pub fn merkle_root(&self) -> H256 {
		merkle_root(&self.transactions.iter().map(|tx| tx.hash.clone()).collect::<Vec<_>>())
	}

	pub fn is_final(&self, height: u32) -> bool {
		self.transactions.iter().all(|tx| tx.raw.is_final(height, self.header.raw.time))
	}
}
