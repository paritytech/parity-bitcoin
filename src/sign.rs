use script::{Sighash, SighashBase, Script, Builder};
use keys::KeyPair;
use crypto::dhash256;
use hash::{H256, h256_from_u8};
use stream::Stream;
use transaction::{Transaction, TransactionOutput, OutPoint, TransactionInput};

pub struct UnsignedTransactionInput {
	pub previous_output: OutPoint,
	pub sequence: u32,
}

pub struct TransactionInputSigner {
	pub version: i32,
	pub inputs: Vec<UnsignedTransactionInput>,
	pub outputs: Vec<TransactionOutput>,
	pub lock_time: u32,
}

impl TransactionInputSigner {
	pub fn signature_hash(&self, input_index: usize, script_pubkey: &Script, sighash: Sighash) -> H256 {
		if input_index >= self.inputs.len() {
			return h256_from_u8(1);
		}

		if sighash.base == SighashBase::Single && input_index >= self.outputs.len() {
			return h256_from_u8(1);
		}

		let inputs = if sighash.anyone_can_pay {
			let input = &self.inputs[input_index];
			vec![TransactionInput {
				previous_output: input.previous_output.clone(),
				script_sig: script_pubkey.to_vec(),
				sequence: input.sequence,
			}]
		} else {
			self.inputs.iter()
				.enumerate()
				.map(|(n, input)| TransactionInput {
					previous_output: input.previous_output.clone(),
					script_sig: match n == input_index {
						true => script_pubkey.to_vec(),
						false => Vec::new(),
					},
					sequence: match sighash.base {
						SighashBase::Single | SighashBase::None if n != input_index => 0,
						_ => input.sequence,
					},
				})
				.collect()
		};

		let outputs = match sighash.base {
			SighashBase::All => self.outputs.clone(),
			SighashBase::Single => self.outputs.iter()
				.take(input_index + 1)
				.enumerate()
				.map(|(n, out)| match n == input_index {
					true => out.clone(),
					false => TransactionOutput::default(),
				})
				.collect(),
			SighashBase::None => Vec::new(),
		};

		let tx = Transaction {
			transaction_inputs: inputs,
			transaction_outputs: outputs,
			version: self.version,
			lock_time: self.lock_time,
		};

		let mut stream = Stream::default();
		stream.append(&tx);
		stream.append(&u32::from(sighash));
		dhash256(&stream.out())
	}

	/// input_index - index of input to sign
	/// script_pubkey - script_pubkey of input's previous_output pubkey
	pub fn signed_input(
		&self,
		keypair: &KeyPair,
		input_index: usize,
		script_pubkey: &Script,
		sighash: Sighash
	) -> TransactionInput {
		let hash = self.signature_hash(input_index, script_pubkey, sighash);

		let mut signature: Vec<u8> = keypair.private().sign(&hash).unwrap().into();
		signature.push(u32::from(sighash) as u8);
		let script_sig = Builder::default()
			.push_data(&signature)
			.push_data(keypair.public())
			.into_script();

		let unsigned_input = &self.inputs[input_index];
		TransactionInput {
			previous_output: unsigned_input.previous_output.clone(),
			sequence: unsigned_input.sequence,
			script_sig: script_sig.to_vec(),
		}
	}
}


