use chain::Transaction;
use db::TransactionOutputProvider;
use script::Script;

/// Counts signature operations in given transaction
/// bip16_active flag indicates if we should also count signature operations
/// in previous transactions. If one of the previous transaction outputs is
/// missing, we simply ignore that fact and just carry on counting
pub fn transaction_sigops(
	transaction: &Transaction,
	store: &TransactionOutputProvider,
	bip16_active: bool
) -> usize {
	let output_sigops: usize = transaction.outputs.iter().map(|output| {
		let output_script: Script = output.script_pubkey.clone().into();
		output_script.sigops_count(false)
	}).sum();

	if transaction.is_coinbase() {
		return output_sigops;
	}

	let mut input_sigops = 0usize;
	let mut bip16_sigops = 0usize;

	for input in &transaction.inputs {
		let input_script: Script = input.script_sig.clone().into();
		input_sigops += input_script.sigops_count(false);
		if bip16_active {
			let previous_output = match store.transaction_output(&input.previous_output, usize::max_value()) {
				Some(output) => output,
				None => continue,
			};
			let prevout_script: Script = previous_output.script_pubkey.into();
			bip16_sigops += input_script.pay_to_script_hash_sigops(&prevout_script);
		}
	}

	input_sigops + output_sigops + bip16_sigops
}
