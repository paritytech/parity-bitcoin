//! Bitcoin trainsaction.
//! https://en.bitcoin.it/wiki/Protocol_documentation#tx

use std::io;
use heapsize::HeapSizeOf;
use hex::FromHex;
use bytes::Bytes;
use ser::{
	Deserializable, Reader, Error as ReaderError, deserialize,
	Serializable, Stream, serialize, serialized_list_size
};
use crypto::dhash256;
use hash::H256;
use constants::{SEQUENCE_FINAL, LOCKTIME_THRESHOLD};

#[derive(Debug, PartialEq, Eq, Clone, Default)]
pub struct OutPoint {
	pub hash: H256,
	pub index: u32,
}

impl Serializable for OutPoint {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.hash)
			.append(&self.index);
	}

	#[inline]
	fn serialized_size(&self) -> usize {
		self.hash.serialized_size() + self.index.serialized_size()
	}
}

impl Deserializable for OutPoint {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = OutPoint {
			hash: try!(reader.read()),
			index: try!(reader.read()),
		};

		Ok(result)
	}
}

impl OutPoint {
	pub fn null() -> Self {
		OutPoint {
			hash: H256::default(),
			index: u32::max_value(),
		}
	}

	pub fn hash(&self) -> &H256 {
		&self.hash
	}

	pub fn is_null(&self) -> bool {
		self.hash.is_zero() && self.index == u32::max_value()
	}
}

#[derive(Debug, PartialEq, Default, Clone)]
pub struct TransactionInput {
	pub previous_output: OutPoint,
	pub script_sig: Bytes,
	pub sequence: u32,
}

impl TransactionInput {
	pub fn is_final(&self) -> bool {
		self.sequence == SEQUENCE_FINAL
	}
}

impl Serializable for TransactionInput {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.previous_output)
			.append(&self.script_sig)
			.append(&self.sequence);
	}

	#[inline]
	fn serialized_size(&self) -> usize {
		self.previous_output.serialized_size() +
			self.script_sig.serialized_size() +
			self.sequence.serialized_size()
	}
}

impl Deserializable for TransactionInput {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = TransactionInput {
			previous_output: try!(reader.read()),
			script_sig: try!(reader.read()),
			sequence: try!(reader.read()),
		};

		Ok(result)
	}
}

impl HeapSizeOf for TransactionInput {
	fn heap_size_of_children(&self) -> usize {
		self.script_sig.heap_size_of_children()
	}
}

#[derive(Debug, PartialEq, Clone)]
pub struct TransactionOutput {
	pub value: u64,
	pub script_pubkey: Bytes,
}

impl Serializable for TransactionOutput {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.value)
			.append(&self.script_pubkey);
	}

	#[inline]
	fn serialized_size(&self) -> usize {
		self.value.serialized_size() +
			self.script_pubkey.serialized_size()
	}
}

impl Deserializable for TransactionOutput {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = TransactionOutput {
			value: try!(reader.read()),
			script_pubkey: try!(reader.read()),
		};

		Ok(result)
	}
}

impl Default for TransactionOutput {
	fn default() -> Self {
		TransactionOutput {
			value: 0xffffffffffffffffu64,
			script_pubkey: Bytes::default(),
		}
	}
}

impl HeapSizeOf for TransactionOutput {
	fn heap_size_of_children(&self) -> usize {
		self.script_pubkey.heap_size_of_children()
	}
}

#[derive(Debug, PartialEq, Default, Clone)]
pub struct Transaction {
	pub version: i32,
	pub inputs: Vec<TransactionInput>,
	pub outputs: Vec<TransactionOutput>,
	pub lock_time: u32,
}

impl Serializable for Transaction {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append_list(&self.inputs)
			.append_list(&self.outputs)
			.append(&self.lock_time);
	}

	#[inline]
	fn serialized_size(&self) -> usize {
		self.version.serialized_size() +
			serialized_list_size(&self.inputs) +
			serialized_list_size(&self.outputs) +
			self.lock_time.serialized_size()
	}
}

impl Deserializable for Transaction {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = Transaction {
			version: try!(reader.read()),
			inputs: try!(reader.read_list()),
			outputs: try!(reader.read_list()),
			lock_time: try!(reader.read()),
		};

		Ok(result)
	}
}

impl From<&'static str> for Transaction {
	fn from(s: &'static str) -> Self {
		deserialize(&s.from_hex().unwrap() as &[u8]).unwrap()
	}
}

impl HeapSizeOf for Transaction {
	fn heap_size_of_children(&self) -> usize {
		self.inputs.heap_size_of_children() + self.outputs.heap_size_of_children()
	}
}

impl Transaction {
	pub fn hash(&self) -> H256 {
		dhash256(&serialize(self))
	}

	pub fn inputs(&self) -> &[TransactionInput] {
		&self.inputs
	}

	pub fn outputs(&self) -> &[TransactionOutput] {
		&self.outputs
	}

	pub fn is_empty(&self) -> bool {
		self.inputs.is_empty() || self.outputs.is_empty()
	}

	pub fn is_null(&self) -> bool {
		self.inputs.iter().any(|input| input.previous_output.is_null())
	}

	pub fn is_coinbase(&self) -> bool {
		self.inputs.len() == 1 && self.inputs[0].previous_output.is_null()
	}

	pub fn is_final(&self, block_height: u32, block_time: u32) -> bool {
		if self.lock_time == 0 {
			return true;
		}

		let max_lock_time = if self.lock_time < LOCKTIME_THRESHOLD {
			block_height
		} else {
			block_time
		};

		if self.lock_time < max_lock_time {
			return true;
		}

		self.inputs.iter().all(TransactionInput::is_final)
	}

	pub fn total_spends(&self) -> u64 {
		self.outputs.iter().map(|output| output.value).sum()
	}
}

#[cfg(test)]
mod tests {
	use hash::H256;
	use ser::Serializable;
	use super::Transaction;

	// real transaction from block 80000
	// https://blockchain.info/rawtx/5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2
	// https://blockchain.info/rawtx/5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2?format=hex
	#[test]
	fn test_transaction_reader() {
		let t: Transaction = "0100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000".into();
		assert_eq!(t.version, 1);
		assert_eq!(t.lock_time, 0);
		assert_eq!(t.inputs.len(), 1);
		assert_eq!(t.outputs.len(), 1);
		let tx_input = &t.inputs[0];
		assert_eq!(tx_input.sequence, 4294967295);
		assert_eq!(tx_input.script_sig, "48304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501".into());
		let tx_output = &t.outputs[0];
		assert_eq!(tx_output.value, 5000000000);
		assert_eq!(tx_output.script_pubkey, "76a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac".into());
	}

	#[test]
	fn test_transaction_hash() {
		let t: Transaction = "0100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000".into();
		let hash = H256::from_reversed_str("5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2");
		assert_eq!(t.hash(), hash);
	}

	#[test]
	fn test_transaction_serialized_len() {
		let raw_tx: &'static str = "0100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000";
		let tx: Transaction = raw_tx.into();
		assert_eq!(tx.serialized_size(), raw_tx.len() / 2);
	}
}
