use hash::H256;
use ser::{Serializable, Stream, Deserializable, Reader, Error as ReaderError};

#[derive(Debug, PartialEq)]
pub struct GetBlocks {
	version: u32,
	block_locator_hashes: Vec<H256>,
	hash_stop: H256,
}

impl Serializable for GetBlocks {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append_list(&self.block_locator_hashes)
			.append(&self.hash_stop);
	}
}

impl Deserializable for GetBlocks {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let get_blocks = GetBlocks {
			version: try!(reader.read()),
			block_locator_hashes: try!(reader.read_list()),
			hash_stop: try!(reader.read()),
		};

		Ok(get_blocks)
	}
}

