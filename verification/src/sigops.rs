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
}

pub fn transaction_sigops(
	transaction: &Transaction,
	store: &PreviousTransactionOutputProvider,
	bip16_active: bool
) -> usize {
	let output_sigops: usize = transaction.outputs.iter().map(|output| {
		let output_script: Script = output.script_pubkey.clone().into();
		output_script.sigops_count(false)
	}).sum();

	if transaction.is_coinbase() {
		return output_sigops;
	}

	let input_sigops: usize = transaction.inputs.iter().map(|input| {
		let input_script: Script = input.script_sig.clone().into();
		let mut sigops = input_script.sigops_count(false);
		if bip16_active {
			let previous_output = store.previous_transaction_output(&input.previous_output)
				.expect("missing tx, out of order verification or malformed db");
			let prevout_script: Script = previous_output.script_pubkey.into();
			sigops += input_script.pay_to_script_hash_sigops(&prevout_script);
		}
		sigops
	}).sum();

	input_sigops + output_sigops
}
