use block_header::BlockHeader;
use compact_integer::CompactInteger;
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
