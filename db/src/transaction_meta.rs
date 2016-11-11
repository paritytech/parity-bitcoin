//! Transaction index

use bit_vec::BitVec;
use byteorder::{LittleEndian, ByteOrder};

/// structure for indexing transaction info
#[derive(Debug, Clone)]
pub struct TransactionMeta {
	block_height: u32,
	// first bit is coinbase flag, others - one per output listed
	bits: BitVec,
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
			bits: BitVec::from_elem(outputs + 1, false),
		}
	}

	/// note that particular output has been used
	pub fn note_used(&mut self, index: usize) {
		self.bits.set(index + 1 , true);
	}

	pub fn coinbase(mut self) -> Self {
		self.bits.set(0, true);
		self
	}

	pub fn is_coinbase(&self) -> bool {
		self.bits.get(0)
			.expect("One bit should always exists, since it is created as usize + 1; minimum value of usize is 0; 0 + 1 = 1;  qed")
	}

	/// note that particular output has been used
	pub fn denote_used(&mut self, index: usize) {
		self.bits.set(index + 1, false);
	}

	pub fn into_bytes(self) -> Vec<u8> {
		let mut result = vec![0u8; 4];
		LittleEndian::write_u32(&mut result[0..4], self.block_height);
		result.extend(self.bits.to_bytes());
		result
	}

	pub fn from_bytes(bytes: &[u8]) -> Result<TransactionMeta, Error> {
		if bytes.len() <= 4 { return Err(Error::KeyTooShort(bytes.len())); }

		Ok(TransactionMeta {
			block_height: LittleEndian::read_u32(&bytes[0..4]),
			bits: BitVec::from_bytes(&bytes[4..]),
		})
	}

	pub fn height(&self) -> u32 { self.block_height }

	pub fn is_spent(&self, idx: usize) -> bool { self.bits.get(idx + 1).expect("Index should be verified by the caller") }

}
