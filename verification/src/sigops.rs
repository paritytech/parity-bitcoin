use network::ConsensusFork;
use chain::Transaction;
use storage::TransactionOutputProvider;
use script::{Script, ScriptWitness};

/// Counts signature operations in given transaction
/// bip16_active flag indicates if we should also count signature operations
/// in previous transactions. If one of the previous transaction outputs is
/// missing, we simply ignore that fact and just carry on counting
pub fn transaction_sigops(
	transaction: &Transaction,
	store: &dyn TransactionOutputProvider,
	bip16_active: bool,
	checkdatasig_active: bool,
) -> usize {
	let output_sigops: usize = transaction.outputs.iter().map(|output| {
		let output_script: Script = output.script_pubkey.clone().into();
		output_script.sigops_count(checkdatasig_active, false)
	}).sum();

	// TODO: bitcoin/bitcoin also includes input_sigops here
	if transaction.is_coinbase() {
		return output_sigops;
	}

	let mut input_sigops = 0usize;
	let mut bip16_sigops = 0usize;

	for input in &transaction.inputs {
		let input_script: Script = input.script_sig.clone().into();
		input_sigops += input_script.sigops_count(checkdatasig_active, false);
		if bip16_active {
			let previous_output = match store.transaction_output(&input.previous_output, usize::max_value()) {
				Some(output) => output,
				None => continue,
			};
			let prevout_script: Script = previous_output.script_pubkey.into();
			bip16_sigops += input_script.pay_to_script_hash_sigops(checkdatasig_active, &prevout_script);
		}
	}

	input_sigops + output_sigops + bip16_sigops
}

pub fn transaction_sigops_cost(
	transaction: &Transaction,
	store: &dyn TransactionOutputProvider,
	sigops: usize,
) -> usize {
	let sigops_cost = sigops * ConsensusFork::witness_scale_factor();
	let witness_sigops_cost: usize = transaction.inputs.iter()
		.map(|input| store.transaction_output(&input.previous_output, usize::max_value())
			.map(|output| witness_sigops(&Script::new(input.script_sig.clone()), &Script::new(output.script_pubkey.clone()), &input.script_witness,))
			.unwrap_or(0))
		.sum();
	sigops_cost + witness_sigops_cost
}

fn witness_sigops(
	script_sig: &Script,
	script_pubkey: &Script,
	script_witness: &ScriptWitness,
) -> usize {
	if let Some((witness_version, witness_program)) = script_pubkey.parse_witness_program() {
		return witness_program_sigops(witness_version, witness_program, script_witness);
	}

	if script_pubkey.is_pay_to_script_hash() && script_sig.is_push_only() {
		if let Some(Ok(instruction)) = script_sig.iter().last() {
			if let Some(data) = instruction.data {
				let subscript = Script::new(data.into());
				if let Some((witness_version, witness_program)) = subscript.parse_witness_program() {
					return witness_program_sigops(witness_version, witness_program, script_witness);
				}
			}
		}
	}

	0
}

fn witness_program_sigops(
	witness_version: u8,
	witness_program: &[u8],
	script_witness: &ScriptWitness,
) -> usize {
	match witness_version {
		0 if witness_program.len() == 20 => 1,
		0 if witness_program.len() == 32 => match script_witness.last() {
			Some(subscript) => Script::new(subscript.clone()).sigops_count(false, true),
			_ => 0,
		},
		_ => 0,
	}
}
