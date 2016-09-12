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

#[cfg(test)]
mod tests {
	use hex::{ToHex, FromHex};
	use hash::h256_from_str;
	use keys::{KeyPair, Private, Address};
	use transaction::{OutPoint, TransactionOutput};
	use script::{Script, Sighash, SighashBase};
	use super::{UnsignedTransactionInput, TransactionInputSigner};

	// http://www.righto.com/2014/02/bitcoins-hard-way-using-raw-bitcoin.html
	// https://blockchain.info/rawtx/81b4c832d70cb56ff957589752eb4125a4cab78a25a8fc52d6a09e5bd4404d48
	// https://blockchain.info/rawtx/3f285f083de7c0acabd9f106a43ec42687ab0bebe2e6f0d529db696794540fea
	#[test]
	fn sign_transaction_input() {
		let private: Private = "5HusYj2b2x4nroApgfvaSfKYZhRbKFH41bVyPooymbC6KfgSXdD".into();
		let previous_tx_hash = h256_from_str("81b4c832d70cb56ff957589752eb4125a4cab78a25a8fc52d6a09e5bd4404d48");
		let previous_output_index = 0;
		let from: Address = "1MMMMSUb1piy2ufrSguNUdFmAcvqrQF8M5".into();
		let to: Address = "1KKKK6N21XKo48zWKuQKXdvSsCf95ibHFa".into();
		let previous_output = "76a914df3bd30160e6c6145baaf2c88a8844c13a00d1d588ac".into();
		let current_output = "76a914c8e90996c7c6080ee06284600c684ed904d14c5c88ac".from_hex().unwrap();
		let value = 91234;
		let expected_script_sig = "47304402202cb265bf10707bf49346c3515dd3d16fc454618c58ec0a0ff448a676c54ff71302206c6624d762a1fcef4618284ead8f08678ac05b13c84235f1654e6ad168233e8201410414e301b2328f17442c0b8310d787bf3d8a404cfbd0704f135b6ad4b2d3ee751310f981926e53a6e8c39bd7d3fefd576c543cce493cbac06388f2651d1aacbfcd".from_hex().unwrap();

		let unsigned_input = UnsignedTransactionInput {
			sequence: 0xffff_ffff,
			previous_output: OutPoint {
				index: previous_output_index,
				hash: previous_tx_hash,
			},
		};

		let output = TransactionOutput {
			value: value,
			script_pubkey: current_output,
		};

		let input_signer = TransactionInputSigner {
			version: 1,
			lock_time: 0,
			inputs: vec![unsigned_input],
			outputs: vec![output],
		};

		let kp = KeyPair::from_private(private).unwrap();
		let input = input_signer.signed_input(&kp, 0, &previous_output, Sighash::new(SighashBase::All, false));

		println!("input seq: {:?}", input.sequence);
		println!("input outpoint index: {:?}", input.previous_output.index);
		println!("input outpoint hash: {:?}", input.previous_output.hash.to_hex());
		println!("input sig: {:?}", input.script_sig.to_hex());
		assert_eq!(input.script_sig, expected_script_sig);
	}
}
