use std::io;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use chain::{BlockHeader, ShortTransactionID};
use common::PrefilledTransaction;

#[derive(Debug, PartialEq)]
pub struct BlockHeaderAndIDs {
	pub header: BlockHeader,
	pub nonce: u64,
	pub short_ids: Vec<ShortTransactionID>,
	pub prefilled_transactions: Vec<PrefilledTransaction>,
}

impl Serializable for BlockHeaderAndIDs {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.header)
			.append(&self.nonce)
			.append_list(&self.short_ids)
			.append_list(&self.prefilled_transactions);
	}
}

impl Deserializable for BlockHeaderAndIDs {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let header= BlockHeaderAndIDs {
			header: try!(reader.read()),
			nonce: try!(reader.read()),
			short_ids: try!(reader.read_list()),
			prefilled_transactions: try!(reader.read_list()),
		};

		Ok(header)
	}
}
