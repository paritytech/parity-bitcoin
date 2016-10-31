use std::io;
use hash::H256;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use chain::Transaction;

#[derive(Debug, PartialEq)]
pub struct BlockTransactions {
	pub blockhash: H256,
	pub transactions: Vec<Transaction>,
}

impl Serializable for BlockTransactions {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.blockhash)
			.append_list(&self.transactions);
	}
}

impl Deserializable for BlockTransactions {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let block_transactions = BlockTransactions {
			blockhash: try!(reader.read()),
			transactions: try!(reader.read_list()),
		};

		Ok(block_transactions)
	}
}
