use std::{io, cmp};
use hash::H256;
use ser::{
	Serializable, serialized_list_size,
	Deserializable, Reader, Error as ReaderError
};
use block::Block;
use transaction::Transaction;
use merkle_root::merkle_root;
use indexed_header::IndexedBlockHeader;
use indexed_transaction::IndexedTransaction;

#[derive(Debug, Clone)]
pub struct IndexedBlock {
	pub header: IndexedBlockHeader,
	pub transactions: Vec<IndexedTransaction>,
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

impl cmp::PartialEq for IndexedBlock {
	fn eq(&self, other: &Self) -> bool {
		self.header.hash == other.header.hash
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

	pub fn to_raw_block(self) -> Block {
		Block::new(self.header.raw, self.transactions.into_iter().map(|tx| tx.raw).collect())
	}

	pub fn size(&self) -> usize {
		let header_size = self.header.raw.serialized_size();
		let transactions = self.transactions.iter().map(|tx| &tx.raw).collect::<Vec<_>>();
		let txs_size = serialized_list_size::<Transaction, &Transaction>(&transactions);
		header_size + txs_size
	}

	pub fn merkle_root(&self) -> H256 {
		merkle_root(&self.transactions.iter().map(|tx| &tx.hash).collect::<Vec<&H256>>())
	}

	pub fn is_final(&self, height: u32) -> bool {
		self.transactions.iter().all(|tx| tx.raw.is_final_in_block(height, self.header.raw.time))
	}
}

impl Deserializable for IndexedBlock {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let block = IndexedBlock {
			header: try!(reader.read()),
			transactions: try!(reader.read_list()),
		};

		Ok(block)
	}
}
