use chain::{Transaction, TransactionOutput, OutPoint};
use db::{PreviousTransactionOutputProvider, SharedStore};
use script::Script;

pub struct StoreWithUnretainedOutputs<'a, T> where T: 'a {
	store: &'a SharedStore,
	outputs: &'a T,
}

impl<'a, T> StoreWithUnretainedOutputs<'a, T> where T: PreviousTransactionOutputProvider {
	pub fn new(store: &'a SharedStore, outputs: &'a T) -> Self {
		StoreWithUnretainedOutputs {
			store: store,
			outputs: outputs,
		}
	}
}

impl<'a, T> PreviousTransactionOutputProvider for StoreWithUnretainedOutputs<'a, T> where T: PreviousTransactionOutputProvider {
	fn previous_transaction_output(&self, prevout: &OutPoint) -> Option<TransactionOutput> {
		self.store.transaction(&prevout.hash)
			.and_then(|tx| tx.outputs.into_iter().nth(prevout.index as usize))
			.or_else(|| self.outputs.previous_transaction_output(prevout))
	}

	fn is_spent(&self, _prevout: &OutPoint) -> bool {
		unimplemented!();
	}
}

pub fn transaction_sigops(
	transaction: &Transaction,
	store: &PreviousTransactionOutputProvider,
	bip16_active: bool
) -> Option<usize> {
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
		if bip16_active {
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
