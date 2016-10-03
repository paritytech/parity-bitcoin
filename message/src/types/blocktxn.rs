use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use common::BlockTransactions;

#[derive(Debug, PartialEq)]
pub struct BlockTxn {
	request: BlockTransactions,
}

impl Serializable for BlockTxn {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.request);
	}
}

impl Deserializable for BlockTxn {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let block = BlockTxn {
			request: try!(reader.read()),
		};

		Ok(block)
	}
}
