
//! Bitcoin trainsaction.
//! https://en.bitcoin.it/wiki/Protocol_documentation#tx

use reader::{Deserializable, Reader, Error as ReaderError};
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

impl Deserializable for OutPoint {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let mut hash = [0u8; 32];
		hash.copy_from_slice(try!(reader.read_bytes(32)));
		let index = try!(reader.read());
		let result = OutPoint {
			hash: hash,
			index: index,
		};

		Ok(result)
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

impl Deserializable for TransactionInput {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let previous_output = try!(reader.read());
		let signature_script_len = try!(reader.read::<CompactInteger>());
		let signature_script = try!(reader.read_bytes(signature_script_len.into())).to_vec();
		let sequence = try!(reader.read());

		let result = TransactionInput {
			previous_output: previous_output,
			signature_script: signature_script,
			sequence: sequence,
		};

		Ok(result)
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

impl Deserializable for TransactionOutput {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let value = try!(reader.read());
		let pk_script_len = try!(reader.read::<CompactInteger>());
		let pk_script = try!(reader.read_bytes(pk_script_len.into())).to_vec();

		let result = TransactionOutput {
			value: value,
			pk_script: pk_script,
		};

		Ok(result)
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

impl Deserializable for Transaction {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let version = try!(reader.read());
		let tx_inputs_len= try!(reader.read::<CompactInteger>());
		let tx_inputs = try!(reader.read_list(tx_inputs_len.into()));
		let tx_outputs_len= try!(reader.read::<CompactInteger>());
		let tx_outputs = try!(reader.read_list(tx_outputs_len.into()));
		let lock_time = try!(reader.read());

		let result = Transaction {
			version: version,
			transaction_inputs: tx_inputs,
			transaction_outputs: tx_outputs,
			lock_time: lock_time,
		};

		Ok(result)
	}
}
