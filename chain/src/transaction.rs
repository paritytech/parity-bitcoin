//! Bitcoin trainsaction.
//! https://en.bitcoin.it/wiki/Protocol_documentation#tx

use heapsize::HeapSizeOf;
use hex::FromHex;
use bytes::Bytes;
use ser::{deserialize, serialize};
use crypto::dhash256;
use hash::H256;
use constants::{SEQUENCE_FINAL, LOCKTIME_THRESHOLD};

#[derive(Debug, PartialEq, Eq, Clone, Default, Serializable, Deserializable)]
pub struct OutPoint {
	pub hash: H256,
	pub index: u32,
}

impl OutPoint {
	pub fn null() -> Self {
		OutPoint {
			hash: H256::default(),
			index: u32::max_value(),
		}
	}

	pub fn is_null(&self) -> bool {
		self.hash.is_zero() && self.index == u32::max_value()
	}
}

#[derive(Debug, PartialEq, Default, Clone, Serializable, Deserializable)]
pub struct TransactionInput {
	pub previous_output: OutPoint,
	pub script_sig: Bytes,
	pub sequence: u32,
}

impl TransactionInput {
	pub fn coinbase(script_sig: Bytes) -> Self {
		TransactionInput {
			previous_output: OutPoint::null(),
			script_sig: script_sig,
			sequence: SEQUENCE_FINAL,
		}
	}

	pub fn is_final(&self) -> bool {
		self.sequence == SEQUENCE_FINAL
	}
}

impl HeapSizeOf for TransactionInput {
	fn heap_size_of_children(&self) -> usize {
		self.script_sig.heap_size_of_children()
	}
}

#[derive(Debug, PartialEq, Clone, Serializable, Deserializable)]
pub struct TransactionOutput {
	pub value: u64,
	pub script_pubkey: Bytes,
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

#[derive(Debug, PartialEq, Default, Clone, Serializable, Deserializable)]
pub struct Transaction {
	pub version: i32,
	pub inputs: Vec<TransactionInput>,
	pub outputs: Vec<TransactionOutput>,
	pub lock_time: u32,
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

	pub fn is_final(&self) -> bool {
		// if lock_time is 0, transaction is final
		if self.lock_time == 0 {
			return true;
		}
		// setting all sequence numbers to 0xffffffff disables the time lock, so if you want to use locktime,
		// at least one input must have a sequence number below the maximum.
		self.inputs.iter().all(TransactionInput::is_final)
	}

	pub fn is_final_in_block(&self, block_height: u32, block_time: u32) -> bool {
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
		let mut result = 0u64;
		for output in self.outputs.iter() {
			if u64::max_value() - result < output.value {
				return u64::max_value();
			}
			result += output.value;
		}
		result
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
