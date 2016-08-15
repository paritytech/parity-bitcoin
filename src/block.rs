use block_header::BlockHeader;
use compact_integer::CompactInteger;
use reader::{Deserializable, Reader, Error as ReaderError};
use stream::{Serializable, Stream};
use transaction::Transaction;

pub struct Block {
	block_header: BlockHeader,
	transactions: Vec<Transaction>,
}

impl Serializable for Block {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.block_header)
			.append(&CompactInteger::from(self.transactions.len()))
			.append_list(&self.transactions);
	}
}

impl Deserializable for Block {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let block_header = try!(reader.read());
		let tx_len = try!(reader.read::<CompactInteger>());
		let transactions = try!(reader.read_list(tx_len.into()));

		let result = Block {
			block_header: block_header,
			transactions: transactions,
		};

		Ok(result)
	}
}
