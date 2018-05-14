//! Transaction index

use std::io;
use ser::{Serializable, Deserializable, Error as ReaderError, Stream, Reader};

/// structure for indexing transaction info
#[derive(Debug, Clone)]
pub struct TransactionMeta {
	/// is this a coinbase tx?
	is_coinbase: bool,
	/// height of block at which thix tx was canonized
	height: u32,
	/// total number of outputs
	total_outputs: u32,
	/// total number of spent outputs
	spent_outputs: u32,
}

impl Serializable for TransactionMeta {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.is_coinbase)
			.append(&self.height)
			.append(&self.total_outputs)
			.append(&self.spent_outputs);
	}
}

impl Deserializable for TransactionMeta {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = TransactionMeta {
			is_coinbase: reader.read()?,
			height: reader.read()?,
			total_outputs: reader.read()?,
			spent_outputs: reader.read()?,
		};

		Ok(result)
	}
}

impl TransactionMeta {
	/// New transaction description for indexing
	pub fn new(is_coinbase: bool, height: u32, outputs: u32) -> Self {
		TransactionMeta {
			is_coinbase,
			height,
			total_outputs: outputs,
			spent_outputs: 0,
		}
	}

	pub fn is_coinbase(&self) -> bool {
		self.is_coinbase
	}

	pub fn height(&self) -> u32 {
		self.height
	}

	pub fn is_fully_spent(&self) -> bool {
		self.total_outputs == self.spent_outputs
	}

	pub fn add_spent_output(&mut self) {
		self.spent_outputs += 1;
	}

	pub fn remove_spent_output(&mut self) {
		// this implies that db is at least partially correct
		// or else this will panic
		self.spent_outputs -= 1;
	}
}
