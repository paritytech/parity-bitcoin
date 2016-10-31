use std::io;
use hash::H256;
use ser::{
	Serializable, Stream, CompactInteger,
	Deserializable, Reader, Error as ReaderError,
};

#[derive(Debug, PartialEq)]
pub struct BlockTransactionsRequest {
	pub blockhash: H256,
	pub indexes: Vec<usize>,
}

impl Serializable for BlockTransactionsRequest {
	fn serialize(&self, stream: &mut Stream) {
		let indexes: Vec<CompactInteger> = self.indexes
			.iter()
			.map(|x| (*x).into())
			.collect();

		stream
			.append(&self.blockhash)
			.append_list(&indexes);
	}
}

impl Deserializable for BlockTransactionsRequest {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let blockhash = try!(reader.read());
		let indexes: Vec<CompactInteger> = try!(reader.read_list());

		let request = BlockTransactionsRequest {
			blockhash: blockhash,
			indexes: indexes.into_iter().map(Into::into).collect(),
		};

		Ok(request)
	}
}
