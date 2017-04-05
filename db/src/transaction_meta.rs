//! Transaction index

use std::io;
use bit_vec::BitVec;
use bytes::Bytes;
use ser::{Serializable, Deserializable, Error as ReaderError, Stream, Reader};

/// structure for indexing transaction info
#[derive(Debug, Clone)]
pub struct TransactionMeta {
	block_height: u32,
	/// first bit indicate if transaction is a coinbase transaction
	/// next bits indicate if transaction has spend outputs
	bits: BitVec,
}

impl Serializable for TransactionMeta {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.block_height)
			.append(&Bytes::from(self.bits.to_bytes()));
	}
}

impl Deserializable for TransactionMeta {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = TransactionMeta {
			block_height: reader.read()?,
			bits: BitVec::from_bytes(&reader.read::<Bytes>()?),
		};

		Ok(result)
	}
}

impl TransactionMeta {
	/// New transaction description for indexing
	pub fn new(block_height: u32, outputs: usize) -> Self {
		TransactionMeta {
			block_height: block_height,
			bits: BitVec::from_elem(outputs + 1, false),
		}
	}

	/// New coinbase transaction
	pub fn new_coinbase(block_height: u32, outputs: usize) -> Self {
		let mut result = Self::new(block_height, outputs);
		result.bits.set(0, true);
		result
	}

	/// Returns true if it is a coinbase transaction
	pub fn is_coinbase(&self) -> bool {
		self.bits.get(0)
			.expect("One bit should always exists, since it is created as usize + 1; minimum value of usize is 0; 0 + 1 = 1; qed")
	}

	/// Denote particular output as used
	pub fn denote_used(&mut self, index: usize) {
		self.bits.set(index + 1 , true);
	}

	/// Denote particular output as not used
	pub fn denote_unused(&mut self, index: usize) {
		self.bits.set(index + 1, false);
	}

	pub fn height(&self) -> u32 {
		self.block_height
	}

	pub fn is_spent(&self, idx: usize) -> Option<bool> {
		self.bits.get(idx + 1)
	}

	pub fn is_fully_spent(&self) -> bool {
		// skip coinbase bit, the rest needs to true
		self.bits.iter().skip(1).all(|x| x)
	}
}

#[cfg(test)]
mod tests {
	use super::TransactionMeta;

	#[test]
	fn test_is_fully_spent() {
		let t = TransactionMeta::new(0, 0);
		assert!(t.is_fully_spent());

		let mut t = TransactionMeta::new(0, 1);
		assert!(!t.is_fully_spent());
		t.denote_used(0);
		assert!(t.is_fully_spent());
		t.denote_unused(0);
		assert!(!t.is_fully_spent());
	}
}
