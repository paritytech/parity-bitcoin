use script::Sighash;
use keys::KeyPair;
use transaction::{Transaction, TransactionOutput, MutableTransaction};

pub enum Error {
	InvalidIndex,
	InvalidPreviousOutputIndex,
}

pub fn sign_signature(
	key: &KeyPair,
	tx_from: &Transaction,
	tx_to: &MutableTransaction,
	index: usize,
	hashtype: Sighash
) -> Result<Vec<u8>, Error> {
	if index >= tx_to.transaction_inputs.len() {
		return Err(Error::InvalidIndex);
	}

	let input = &tx_to.transaction_inputs[index];
	let n = input.previous_output().index() as usize;
	if n >= tx_from.transaction_outputs().len() {
		return Err(Error::InvalidPreviousOutputIndex);
	}

	let output = &tx_from.transaction_outputs()[n];
	sign_signature_with_output(key, output, tx_to, index, hashtype)
}

pub fn sign_signature_with_output(
	_key: &KeyPair,
	_output: &TransactionOutput,
	tx: &MutableTransaction,
	index: usize,
	_hashtype: Sighash
) -> Result<Vec<u8>, Error> {
	if index >= tx.transaction_inputs.len() {
		return Err(Error::InvalidIndex);
	}

	unimplemented!();
}
