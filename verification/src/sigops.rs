use chain::{Transaction, TransactionOutput, OutPoint};
use db::{PreviousTransactionOutputProvider};
use script::Script;

pub struct StoreWithUnretainedOutputs<'a> {
	store: &'a PreviousTransactionOutputProvider,
	outputs: &'a PreviousTransactionOutputProvider,
}

impl<'a> StoreWithUnretainedOutputs<'a> {
	pub fn new(store: &'a PreviousTransactionOutputProvider, outputs: &'a PreviousTransactionOutputProvider) -> Self {
		StoreWithUnretainedOutputs {
			store: store,
			outputs: outputs,
		}
	}
}

impl<'a> PreviousTransactionOutputProvider for StoreWithUnretainedOutputs<'a> {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.store.previous_transaction_output(prevout)
			.or_else(|| self.outputs.previous_transaction_output(prevout))
	}
}

pub fn transaction_sigops(
	transaction: &Transaction,
	store: &PreviousTransactionOutputProvider,
	bip16_active: bool
) -> Option<usize> {
	if bip16_active {
		transaction_sigops_raw(transaction, Some(store))
	} else {
		transaction_sigops_raw(transaction, None)
	}
}

pub fn transaction_sigops_raw(transaction: &Transaction, store: Option<&PreviousTransactionOutputProvider>) -> Option<usize> {
	let output_sigops: usize = transaction.outputs.iter().map(|output| {
		let output_script: Script = output.script_pubkey.clone().into();
		output_script.sigops_count(false)
	}).sum();

	if transaction.is_coinbase() {
		return Some(output_sigops);
	}

	let mut input_sigops = 0usize;
	let mut bip16_sigops = 0usize;

	for input in &transaction.inputs {
		let input_script: Script = input.script_sig.clone().into();
		input_sigops += input_script.sigops_count(false);
		if let Some(store) = store {
			let previous_output = match store.previous_transaction_output(&input.previous_output) {
				Some(output) => output,
				None => return None,
			};
			let prevout_script: Script = previous_output.script_pubkey.into();
			bip16_sigops += input_script.pay_to_script_hash_sigops(&prevout_script);
		}
	}

	Some(input_sigops + output_sigops + bip16_sigops)
}
