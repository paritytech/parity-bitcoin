//! Transaction index

use bit_vec::BitVec;
use byteorder::{LittleEndian, ByteOrder};

/// structure for indexing transaction info
pub struct TransactionMeta {
	block_height: u32,
	spent: BitVec,
}

#[derive(Debug)]
pub enum Error {
	KeyTooShort(usize),
}

impl TransactionMeta {
	/// new transaction description for indexing
	pub fn new(block_height: u32, outputs: usize) -> Self {
		TransactionMeta {
			block_height: block_height,
			spent: BitVec::from_elem(outputs, false),
		}
	}

	/// note that particular output has been used
	pub fn note_used(&mut self, index: usize) {
		self.spent.set(index, true);
	}

	pub fn to_bytes(self) -> Vec<u8> {
		let mut result = vec![0u8; 4];
		LittleEndian::write_u32(&mut result[0..4], self.block_height);
		result.extend(self.spent.to_bytes());
		result
	}

	pub fn from_bytes(bytes: &[u8]) -> Result<TransactionMeta, Error> {
		if bytes.len() <= 4 { return Err(Error::KeyTooShort(bytes.len())); }

		Ok(TransactionMeta {
			block_height: LittleEndian::read_u32(&bytes[0..4]),
			spent: BitVec::from_bytes(&bytes[5..]),
		})
	}

	pub fn height(&self) -> u32 { self.block_height }

	pub fn is_spent(&self, idx: usize) -> bool { self.spent.get(idx).expect("Index should be verified by the caller") }

}
