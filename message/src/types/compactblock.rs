use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};
use common::BlockHeaderAndIDs;

#[derive(Debug, PartialEq)]
pub struct CompactBlock {
	header: BlockHeaderAndIDs,
}

impl Serializable for CompactBlock {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&self.header);
	}
}

impl Deserializable for CompactBlock {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let block = CompactBlock {
			header: try!(reader.read()),
		};

		Ok(block)
	}
}
