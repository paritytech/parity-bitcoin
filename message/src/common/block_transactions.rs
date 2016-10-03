use hash::H256;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use chain::Transaction;

#[derive(Debug, PartialEq)]
pub struct BlockTransactions {
	blockhash: H256,
	transactions: Vec<Transaction>,
}

impl Serializable for BlockTransactions {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.blockhash)
			.append_list(&self.transactions);
	}
}

impl Deserializable for BlockTransactions {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let block_transactions = BlockTransactions {
			blockhash: try!(reader.read()),
			transactions: try!(reader.read_list()),
		};

		Ok(block_transactions)
	}
}
