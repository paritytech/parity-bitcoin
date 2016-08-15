
//! Bitcoin trainsaction.
//! https://en.bitcoin.it/wiki/Protocol_documentation#tx

use stream::{Serializable, Stream};
use compact_integer::CompactInteger;

#[derive(Debug)]
pub struct OutPoint {
	hash: [u8; 32],
	index: u32,
}

impl Serializable for OutPoint {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append_bytes(&self.hash)
			.append(&self.index);
	}
}

#[derive(Debug)]
pub struct TransactionInput {
	previous_output: OutPoint,
	signature_script: Vec<u8>,
	sequence: u32,
}

impl Serializable for TransactionInput {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.previous_output)
			.append(&CompactInteger::from(self.signature_script.len()))
			.append_bytes(&self.signature_script)
			.append(&self.sequence);
	}
}

#[derive(Debug)]
pub struct TransactionOutput {
	value: u64,
	pk_script: Vec<u8>,
}

impl Serializable for TransactionOutput {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.value)
			.append(&CompactInteger::from(self.pk_script.len()))
			.append_bytes(&self.pk_script);
	}
}

#[derive(Debug)]
pub struct Transaction {
	version: i32,
	transaction_inputs: Vec<TransactionInput>,
	transaction_outputs: Vec<TransactionOutput>,
	lock_time: u32,
}

impl Serializable for Transaction {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append(&CompactInteger::from(self.transaction_inputs.len()))
			.append_list(&self.transaction_inputs)
			.append(&CompactInteger::from(self.transaction_outputs.len()))
			.append_list(&self.transaction_outputs)
			.append(&self.lock_time);
	}
}
