use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use common::BlockTransactionsRequest;

#[derive(Debug, PartialEq)]
pub struct GetBlockTxn {
	request: BlockTransactionsRequest,
}

impl Serializable for GetBlockTxn {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.request);
	}
}

impl Deserializable for GetBlockTxn {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let get_block = GetBlockTxn {
			request: try!(reader.read()),
		};

		Ok(get_block)
	}
}
