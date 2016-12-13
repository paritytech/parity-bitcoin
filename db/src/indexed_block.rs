use std::io;
use primitives::hash::H256;
use chain::{Block, OutPoint, TransactionOutput, merkle_root, Transaction};
use serialization::{
	Serializable, serialized_list_size,
	Deserializable, Reader, Error as ReaderError
};
use indexed_header::IndexedBlockHeader;
use indexed_transaction::IndexedTransaction;
use {TransactionOutputObserver, PreviousTransactionOutputProvider};

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
}

impl TransactionOutputObserver for IndexedBlock {
	fn is_spent(&self, _prevout: &OutPoint) -> Option<bool> {
		// the code below is valid, but commented out due it's poor performance
		// we could optimize it by indexing all outputs once
		// let tx: IndexedTransaction = { .. }
		// let indexed_outputs: IndexedOutputs = tx.indexed_outputs();
		// indexed_outputs.is_spent()
		None

		// if previous transaction output appears more than once than we can safely
		// tell that it's spent (double spent)

		//let spends = self.transactions.iter()
			//.flat_map(|tx| &tx.raw.inputs)
			//.filter(|input| &input.previous_output == prevout)
			//.take(2)
			//.count();

		//match spends {
			//0 => None,
			//1 => Some(false),
			//2 => Some(true),
			//_ => unreachable!("spends <= 2; self.take(2); qed"),
		//}
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
		self.transactions.iter().all(|tx| tx.raw.is_final(height, self.header.raw.time))
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
