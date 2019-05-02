use std::io;
use hash::H32;
use ser::{Deserializable, Reader, Error as ReaderError};
use chain::IndexedBlock;

#[derive(Debug, PartialEq)]
pub struct Block {
	pub magic: H32,
	pub block_size: u32,
	pub block: IndexedBlock,
}

impl Deserializable for Block {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		// We never knew how to parse blocks index file => we were assuming that blocks are stored
		// in the *.blk files next to each other, without any gaps.
		// It seems that this isn't true && there are (sometimes) zero-filled (always???) gaps between
		// adjacent blocks.
		// The straightforward soultion is to skip zero bytes. This will work because magic is designed
		// not to have zero bytes in it AND block is always prefixed with magic.
		reader.skip_while(&|byte| byte == 0)?;

		Ok(Block {
			magic: reader.read()?,
			block_size: reader.read()?,
			block: reader.read()?,
		})
	}
}