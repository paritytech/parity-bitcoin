use std::{cmp, mem};
use bytes::Bytes;
use keys::{Message, Signature, Public};
use chain::constants::SEQUENCE_LOCKTIME_DISABLE_FLAG;
use crypto::{sha1, sha256, dhash160, dhash256, ripemd160};
use sign::{SignatureVersion, Sighash};
use script::MAX_SCRIPT_ELEMENT_SIZE;
use {
	script, Builder, Script, ScriptWitness, Num, VerificationFlags, Opcode, Error, SignatureChecker, Stack
};

/// Helper function.
fn check_signature(
	checker: &dyn SignatureChecker,
	mut script_sig: Vec<u8>,
	public: Vec<u8>,
	script_code: &Script,
	version: SignatureVersion
) -> bool {
	let public = match Public::from_slice(&public) {
		Ok(public) => public,
		_ => return false,
	};

	if script_sig.is_empty() {
		return false;
	}

	let hash_type = script_sig.pop().unwrap() as u32;
	let signature = script_sig.into();

	checker.check_signature(&signature, &public, script_code, hash_type, version)
}

/// Helper function.
fn verify_signature(
	checker: &dyn SignatureChecker,
	signature: Vec<u8>,
	public: Vec<u8>,
	message: Message,
) -> bool {
	let public = match Public::from_slice(&public) {
		Ok(public) => public,
		_ => return false,
	};

	if signature.is_empty() {
		return false;
	}

	checker.verify_signature(&signature.into(), &public, &message.into())
}

fn is_public_key(v: &[u8]) -> bool {
	match v.len() {
		33 if v[0] == 2 || v[0] == 3 => true,
		65 if v[0] == 4 => true,
		_ => false,
	}
}

/// A canonical signature exists of: <30> <total len> <02> <len R> <R> <02> <len S> <S> <hashtype>
/// Where R and S are not negative (their first byte has its highest bit not set), and not
/// excessively padded (do not start with a 0 byte, unless an otherwise negative number follows,
/// in which case a single 0 byte is necessary and even required).
///
/// See https://bitcointalk.org/index.php?topic=8392.msg127623#msg127623
///
/// This function is consensus-critical since BIP66.
fn is_valid_signature_encoding(sig: &[u8]) -> bool {
	// Format: 0x30 [total-length] 0x02 [R-length] [R] 0x02 [S-length] [S] [sighash]
	// * total-length: 1-byte length descriptor of everything that follows,
	//   excluding the sighash byte.
	// * R-length: 1-byte length descriptor of the R value that follows.
	// * R: arbitrary-length big-endian encoded R value. It must use the shortest
	//   possible encoding for a positive integers (which means no null bytes at
	//   the start, except a single one when the next byte has its highest bit set).
	// * S-length: 1-byte length descriptor of the S value that follows.
	// * S: arbitrary-length big-endian encoded S value. The same rules apply.
	// * sighash: 1-byte value indicating what data is hashed (not part of the DER
	//   signature)

	// Minimum and maximum size constraints
	if sig.len() < 9 || sig.len() > 73 {
		return false;
	}

	// A signature is of type 0x30 (compound)
	if sig[0] != 0x30 {
		return false;
	}

	// Make sure the length covers the entire signature.
	if sig[1] as usize != sig.len() - 3 {
		return false;
	}

	// Extract the length of the R element.
	let len_r = sig[3] as usize;

	// Make sure the length of the S element is still inside the signature.
	if len_r + 5 >= sig.len() {
		return false;
	}

	// Extract the length of the S element.
	let len_s = sig[len_r + 5] as usize;

	// Verify that the length of the signature matches the sum of the length
	if len_r + len_s + 7 != sig.len() {
		return false;
	}

	// Check whether the R element is an integer.
	if sig[2] != 2 {
		return false;
	}

	// Zero-length integers are not allowed for R.
	if len_r == 0 {
		return false;
	}

	// Negative numbers are not allowed for R.
	if (sig[4] & 0x80) != 0 {
		return false;
	}

	// Null bytes at the start of R are not allowed, unless R would
	// otherwise be interpreted as a negative number.
	if len_r > 1 && sig[4] == 0 && (sig[5] & 0x80) == 0 {
		return false;
	}

	// Check whether the S element is an integer.
	if sig[len_r + 4] != 2 {
		return false;
	}

	// Zero-length integers are not allowed for S.
	if len_s == 0 {
		return false;
	}

	// Negative numbers are not allowed for S.
	if (sig[len_r + 6] & 0x80) != 0 {
		return false;
	}

	// Null bytes at the start of S are not allowed, unless S would otherwise be
	// interpreted as a negative number.
	if len_s > 1 && (sig[len_r + 6] == 0) && (sig[len_r + 7] & 0x80) == 0 {
		return false;
	}

	true
}

fn is_low_der_signature(sig: &[u8]) -> Result<(), Error> {
	if !is_valid_signature_encoding(sig) {
		return Err(Error::SignatureDer);
	}

	let signature: Signature = sig.into();
	if !signature.check_low_s() {
		return Err(Error::SignatureHighS);
	}

	Ok(())
}

fn is_defined_hashtype_signature(version: SignatureVersion, sig: &[u8]) -> bool {
	if sig.is_empty() {
		return false;
	}

	Sighash::is_defined(version, sig[sig.len() - 1] as u32)
}

fn parse_hash_type(version: SignatureVersion, sig: &[u8]) -> Sighash {
	Sighash::from_u32(version, if sig.is_empty() { 0 } else { sig[sig.len() - 1] as u32 })
}

fn check_signature_encoding(sig: &[u8], flags: &VerificationFlags, version: SignatureVersion) -> Result<(), Error> {
	// Empty signature. Not strictly DER encoded, but allowed to provide a
	// compact way to provide an invalid signature for use with CHECK(MULTI)SIG

	if sig.is_empty() {
		return Ok(());
	}

	if (flags.verify_dersig || flags.verify_low_s || flags.verify_strictenc) && !is_valid_signature_encoding(sig) {
		return Err(Error::SignatureDer);
	}

	if flags.verify_low_s {
		is_low_der_signature(sig)?;
	}

	if flags.verify_strictenc && !is_defined_hashtype_signature(version, sig) {
		return Err(Error::SignatureHashtype)
	}

	// verify_strictenc is currently enabled for BitcoinCash only
	if flags.verify_strictenc {
		let uses_fork_id = parse_hash_type(version, sig).fork_id;
		let enabled_fork_id = version == SignatureVersion::ForkId;
		if uses_fork_id && !enabled_fork_id {
			return Err(Error::SignatureIllegalForkId)
		} else if !uses_fork_id && enabled_fork_id {
			return Err(Error::SignatureMustUseForkId);
		}
	}

	Ok(())
}

fn check_pubkey_encoding(v: &[u8], flags: &VerificationFlags) -> Result<(), Error> {
	if flags.verify_strictenc && !is_public_key(v) {
		return Err(Error::PubkeyType);
	}

	Ok(())
}

fn check_minimal_push(data: &[u8], opcode: Opcode) -> bool {
	if data.is_empty() {
		// Could have used OP_0.
		opcode == Opcode::OP_0
	} else if data.len() == 1 && data[0] >= 1 && data[0] <= 16 {
		// Could have used OP_1 .. OP_16.
		opcode as u8 == Opcode::OP_1 as u8 + (data[0] - 1)
	} else if data.len() == 1 && data[0] == 0x81 {
		// Could have used OP_1NEGATE
		opcode == Opcode::OP_1NEGATE
	} else if data.len() <= 75 {
		// Could have used a direct push (opcode indicating number of bytes pushed + those bytes).
		opcode as usize == data.len()
	} else if data.len() <= 255 {
		// Could have used OP_PUSHDATA.
		opcode == Opcode::OP_PUSHDATA1
	} else if data.len() <= 65535 {
		// Could have used OP_PUSHDATA2.
		opcode == Opcode::OP_PUSHDATA2
	} else {
		true
	}
}

fn cast_to_bool(data: &[u8]) -> bool {
	if data.is_empty() {
		return false;
	}

	if data[..data.len() - 1].iter().any(|x| x != &0) {
		return true;
	}

	let last = data[data.len() - 1];
	!(last == 0 || last == 0x80)
}

/// Verifies script signature and pubkey
pub fn verify_script(
	script_sig: &Script,
	script_pubkey: &Script,
	witness: &ScriptWitness,
	flags: &VerificationFlags,
	checker: &dyn SignatureChecker,
	version: SignatureVersion,
) -> Result<(), Error> {
	if flags.verify_sigpushonly && !script_sig.is_push_only() {
		return Err(Error::SignaturePushOnly);
	}

	let mut stack = Stack::new();
	let mut stack_copy = Stack::new();
	let mut had_witness = false;

	eval_script(&mut stack, script_sig, flags, checker, version)?;

	if flags.verify_p2sh {
		stack_copy = stack.clone();
	}

	let res = eval_script(&mut stack, script_pubkey, flags, checker, version)?;
	if !res {
		return Err(Error::EvalFalse);
	}

	// Verify witness program
	let mut verify_cleanstack = flags.verify_cleanstack;
	if flags.verify_witness {
		if let Some((witness_version, witness_program)) = script_pubkey.parse_witness_program() {
			if !script_sig.is_empty() {
				return Err(Error::WitnessMalleated);
			}

			had_witness = true;
			verify_cleanstack = false;
			if !verify_witness_program(witness, witness_version, witness_program, flags, checker)? {
				return Err(Error::EvalFalse);
			}
		}
	}

	// Additional validation for spend-to-script-hash transactions:
	if flags.verify_p2sh && script_pubkey.is_pay_to_script_hash() {
		if !script_sig.is_push_only() {
			return Err(Error::SignaturePushOnly);
		}

		mem::swap(&mut stack, &mut stack_copy);

		// stack cannot be empty here, because if it was the
		// P2SH  HASH <> EQUAL  scriptPubKey would be evaluated with
		// an empty stack and the EvalScript above would return false.
		assert!(!stack.is_empty());

		let pubkey2: Script = stack.pop()?.into();

		let res = eval_script(&mut stack, &pubkey2, flags, checker, version)?;
		if !res {
			return Err(Error::EvalFalse);
		}

		if flags.verify_witness {
			if let Some((witness_version, witness_program)) = pubkey2.parse_witness_program() {
				if script_sig != &Builder::default().push_data(&pubkey2).into_script() {
					return Err(Error::WitnessMalleatedP2SH);
				}

				had_witness = true;
				verify_cleanstack = false;
				if !verify_witness_program(witness, witness_version, witness_program, flags, checker)? {
					return Err(Error::EvalFalse);
				}
			}
		}
	}

    // The CLEANSTACK check is only performed after potential P2SH evaluation,
    // as the non-P2SH evaluation of a P2SH script will obviously not result in
    // a clean stack (the P2SH inputs remain). The same holds for witness evaluation.
	if verify_cleanstack {
        // Disallow CLEANSTACK without P2SH, as otherwise a switch CLEANSTACK->P2SH+CLEANSTACK
        // would be possible, which is not a softfork (and P2SH should be one).
		assert!(flags.verify_p2sh);
		if stack.len() != 1 {
			return Err(Error::Cleanstack);
		}
	}

	if flags.verify_witness {
		// We can't check for correct unexpected witness data if P2SH was off, so require
		// that WITNESS implies P2SH. Otherwise, going from WITNESS->P2SH+WITNESS would be
		// possible, which is not a softfork.
		assert!(flags.verify_p2sh);
		if !had_witness && !witness.is_empty() {
			return Err(Error::WitnessUnexpected);
		}
	}

	Ok(())
}

fn verify_witness_program(
	witness: &ScriptWitness,
	witness_version: u8,
	witness_program: &[u8],
	flags: &VerificationFlags,
	checker: &dyn SignatureChecker,
) -> Result<bool, Error> {
	if witness_version != 0 {
		if flags.verify_discourage_upgradable_witness_program {
			return Err(Error::DiscourageUpgradableWitnessProgram);
		}

		return Ok(true);
	}

	let witness_stack = witness;
	let witness_stack_len = witness_stack.len();
	let (mut stack, script_pubkey): (Stack<_>, Script) = match witness_program.len() {
		32 => {
			if witness_stack_len == 0 {
				return Err(Error::WitnessProgramWitnessEmpty);
			}

			let script_pubkey = &witness_stack[witness_stack_len - 1];
			let stack = &witness_stack[0..witness_stack_len - 1];
			let script_pubkey_hash = sha256(script_pubkey);

			if script_pubkey_hash != witness_program[0..32].into() {
				return Err(Error::WitnessProgramMismatch);
			}

			(stack.iter().cloned().collect::<Vec<_>>().into(), Script::new(script_pubkey.clone()))
		},
		20 => {
			if witness_stack_len != 2 {
				return Err(Error::WitnessProgramMismatch);
			}

			let script_pubkey = Builder::default()
				.push_opcode(Opcode::OP_DUP)
				.push_opcode(Opcode::OP_HASH160)
				.push_data(witness_program)
				.push_opcode(Opcode::OP_EQUALVERIFY)
				.push_opcode(Opcode::OP_CHECKSIG)
				.into_script();

			(witness_stack.clone().into(), script_pubkey)
		},
		_ => return Err(Error::WitnessProgramWrongLength),
	};

	if stack.iter().any(|s| s.len() > MAX_SCRIPT_ELEMENT_SIZE) {
		return Err(Error::PushSize);
	}

	if !eval_script(&mut stack, &script_pubkey, flags, checker, SignatureVersion::WitnessV0)? {
		return Ok(false);
	}

	if stack.len() != 1 {
		return Err(Error::EvalFalse);
	}

	let success = cast_to_bool(stack.last().expect("stack.len() == 1; last() only returns errors when stack is empty; qed"));
	Ok(success)
}

/// Evaluautes the script
#[cfg_attr(feature="cargo-clippy", allow(match_same_arms))]
pub fn eval_script(
	stack: &mut Stack<Bytes>,
	script: &Script,
	flags: &VerificationFlags,
	checker: &dyn SignatureChecker,
	version: SignatureVersion
) -> Result<bool, Error> {
	if script.len() > script::MAX_SCRIPT_SIZE {
		return Err(Error::ScriptSize);
	}

	let mut pc = 0;
	let mut op_count = 0;
	let mut begincode = 0;
	let mut exec_stack = Vec::<bool>::new();
	let mut altstack = Stack::<Bytes>::new();

	while pc < script.len() {
		let executing = exec_stack.iter().all(|x| *x);
		let instruction = match script.get_instruction(pc) {
			Ok(i) => i,
			Err(Error::BadOpcode) if !executing => {
				pc += 1;
				continue;
			},
			Err(err) => return Err(err),
		};
		let opcode = instruction.opcode;

		if let Some(data) = instruction.data {
			if data.len() > script::MAX_SCRIPT_ELEMENT_SIZE {
				return Err(Error::PushSize);
			}

			if executing && flags.verify_minimaldata && !check_minimal_push(data, opcode) {
				return Err(Error::Minimaldata);
			}
		}

		if opcode.is_countable() {
			op_count += 1;
			if op_count > script::MAX_OPS_PER_SCRIPT {
				return Err(Error::OpCount);
			}
		}

		if opcode.is_disabled(flags) {
			return Err(Error::DisabledOpcode(opcode));
		}

		pc += instruction.step;
		if !(executing || (Opcode::OP_IF <= opcode && opcode <= Opcode::OP_ENDIF)) {
			continue;
		}

		match opcode {
			Opcode::OP_PUSHDATA1 |
			Opcode::OP_PUSHDATA2 |
			Opcode::OP_PUSHDATA4 |
			Opcode::OP_0 |
			Opcode::OP_PUSHBYTES_1 |
			Opcode::OP_PUSHBYTES_2 |
			Opcode::OP_PUSHBYTES_3 |
			Opcode::OP_PUSHBYTES_4 |
			Opcode::OP_PUSHBYTES_5 |
			Opcode::OP_PUSHBYTES_6 |
			Opcode::OP_PUSHBYTES_7 |
			Opcode::OP_PUSHBYTES_8 |
			Opcode::OP_PUSHBYTES_9 |
			Opcode::OP_PUSHBYTES_10 |
			Opcode::OP_PUSHBYTES_11 |
			Opcode::OP_PUSHBYTES_12 |
			Opcode::OP_PUSHBYTES_13 |
			Opcode::OP_PUSHBYTES_14 |
			Opcode::OP_PUSHBYTES_15 |
			Opcode::OP_PUSHBYTES_16 |
			Opcode::OP_PUSHBYTES_17 |
			Opcode::OP_PUSHBYTES_18 |
			Opcode::OP_PUSHBYTES_19 |
			Opcode::OP_PUSHBYTES_20 |
			Opcode::OP_PUSHBYTES_21 |
			Opcode::OP_PUSHBYTES_22 |
			Opcode::OP_PUSHBYTES_23 |
			Opcode::OP_PUSHBYTES_24 |
			Opcode::OP_PUSHBYTES_25 |
			Opcode::OP_PUSHBYTES_26 |
			Opcode::OP_PUSHBYTES_27 |
			Opcode::OP_PUSHBYTES_28 |
			Opcode::OP_PUSHBYTES_29 |
			Opcode::OP_PUSHBYTES_30 |
			Opcode::OP_PUSHBYTES_31 |
			Opcode::OP_PUSHBYTES_32 |
			Opcode::OP_PUSHBYTES_33 |
			Opcode::OP_PUSHBYTES_34 |
			Opcode::OP_PUSHBYTES_35 |
			Opcode::OP_PUSHBYTES_36 |
			Opcode::OP_PUSHBYTES_37 |
			Opcode::OP_PUSHBYTES_38 |
			Opcode::OP_PUSHBYTES_39 |
			Opcode::OP_PUSHBYTES_40 |
			Opcode::OP_PUSHBYTES_41 |
			Opcode::OP_PUSHBYTES_42 |
			Opcode::OP_PUSHBYTES_43 |
			Opcode::OP_PUSHBYTES_44 |
			Opcode::OP_PUSHBYTES_45 |
			Opcode::OP_PUSHBYTES_46 |
			Opcode::OP_PUSHBYTES_47 |
			Opcode::OP_PUSHBYTES_48 |
			Opcode::OP_PUSHBYTES_49 |
			Opcode::OP_PUSHBYTES_50 |
			Opcode::OP_PUSHBYTES_51 |
			Opcode::OP_PUSHBYTES_52 |
			Opcode::OP_PUSHBYTES_53 |
			Opcode::OP_PUSHBYTES_54 |
			Opcode::OP_PUSHBYTES_55 |
			Opcode::OP_PUSHBYTES_56 |
			Opcode::OP_PUSHBYTES_57 |
			Opcode::OP_PUSHBYTES_58 |
			Opcode::OP_PUSHBYTES_59 |
			Opcode::OP_PUSHBYTES_60 |
			Opcode::OP_PUSHBYTES_61 |
			Opcode::OP_PUSHBYTES_62 |
			Opcode::OP_PUSHBYTES_63 |
			Opcode::OP_PUSHBYTES_64 |
			Opcode::OP_PUSHBYTES_65 |
			Opcode::OP_PUSHBYTES_66 |
			Opcode::OP_PUSHBYTES_67 |
			Opcode::OP_PUSHBYTES_68 |
			Opcode::OP_PUSHBYTES_69 |
			Opcode::OP_PUSHBYTES_70 |
			Opcode::OP_PUSHBYTES_71 |
			Opcode::OP_PUSHBYTES_72 |
			Opcode::OP_PUSHBYTES_73 |
			Opcode::OP_PUSHBYTES_74 |
			Opcode::OP_PUSHBYTES_75 => {
				if let Some(data) = instruction.data {
					stack.push(data.to_vec().into());
				}
			},
			Opcode::OP_1NEGATE |
			Opcode::OP_1 |
			Opcode::OP_2 |
			Opcode::OP_3 |
			Opcode::OP_4 |
			Opcode::OP_5 |
			Opcode::OP_6 |
			Opcode::OP_7 |
			Opcode::OP_8 |
			Opcode::OP_9 |
			Opcode::OP_10 |
			Opcode::OP_11 |
			Opcode::OP_12 |
			Opcode::OP_13 |
			Opcode::OP_14 |
			Opcode::OP_15 |
			Opcode::OP_16 => {
				let value = (opcode as i32).wrapping_sub(Opcode::OP_1 as i32 - 1);
				stack.push(Num::from(value).to_bytes());
			},
			Opcode::OP_CAT if flags.verify_concat => {
				let mut value_to_append = stack.pop()?;
				let value_to_update = stack.last_mut()?;
				if value_to_update.len() + value_to_append.len() > script::MAX_SCRIPT_ELEMENT_SIZE {
					return Err(Error::PushSize);
				}
				value_to_update.append(&mut value_to_append);
			},
			// OP_SPLIT replaces OP_SUBSTR
			Opcode::OP_SUBSTR if flags.verify_split => {
				let n = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if n.is_negative() {
					return Err(Error::InvalidStackOperation);
				}
				let n: usize = n.into();
				let splitted_value = {
					let value_to_split = stack.last_mut()?;
					if n > value_to_split.len() {
						return Err(Error::InvalidSplitRange);
					}
					value_to_split.split_off(n)
				};
				stack.push(splitted_value);
			},
			Opcode::OP_AND if flags.verify_and => {
				let mask = stack.pop()?;
				let mask_len = mask.len();
				let value_to_update = stack.last_mut()?;
				if mask_len != value_to_update.len() {
					return Err(Error::InvalidOperandSize);
				}
				for (byte_to_update, byte_mask) in (*value_to_update).iter_mut().zip(mask.iter()) {
					*byte_to_update = *byte_to_update & byte_mask;
				}
			},
			Opcode::OP_OR if flags.verify_or => {
				let mask = stack.pop()?;
				let mask_len = mask.len();
				let value_to_update = stack.last_mut()?;
				if mask_len != value_to_update.len() {
					return Err(Error::InvalidOperandSize);
				}
				for (byte_to_update, byte_mask) in (*value_to_update).iter_mut().zip(mask.iter()) {
					*byte_to_update = *byte_to_update | byte_mask;
				}
			},
			Opcode::OP_XOR if flags.verify_xor => {
				let mask = stack.pop()?;
				let mask_len = mask.len();
				let value_to_update = stack.last_mut()?;
				if mask_len != value_to_update.len() {
					return Err(Error::InvalidOperandSize);
				}
				for (byte_to_update, byte_mask) in (*value_to_update).iter_mut().zip(mask.iter()) {
					*byte_to_update = *byte_to_update ^ byte_mask;
				}
			},
			Opcode::OP_DIV if flags.verify_div => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if v2.is_zero() {
					return Err(Error::DivisionByZero);
				}
				stack.push((v1 / v2).to_bytes());
			},
			Opcode::OP_MOD if flags.verify_mod => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if v2.is_zero() {
					return Err(Error::DivisionByZero);
				}
				stack.push((v1 % v2).to_bytes());
			},
			// OP_BIN2NUM replaces OP_RIGHT
			Opcode::OP_RIGHT if flags.verify_bin2num => {
				let bin = stack.pop()?;
				let n = Num::minimally_encode(&bin, 4)?;
				stack.push(n.to_bytes());
			},
			// OP_NUM2BIN replaces OP_LEFT
			Opcode::OP_LEFT if flags.verify_num2bin => {
				let bin_size = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if bin_size.is_negative() || bin_size > MAX_SCRIPT_ELEMENT_SIZE.into() {
					return Err(Error::PushSize);
				}

				let bin_size: usize = bin_size.into();
				let num = Num::minimally_encode(&stack.pop()?, 4)?;
				let mut num = num.to_bytes();

				// check if we can fit number into array of bin_size length
				if num.len() > bin_size {
					return Err(Error::ImpossibleEncoding);
				}

				// check if we need to extend binary repr with zero-bytes
				if num.len() < bin_size {
					let sign_byte = num.last_mut().map(|last_byte| {
						let sign_byte = *last_byte & 0x80;
						*last_byte = *last_byte & 0x7f;
						sign_byte
					}).unwrap_or(0x00);

					num.resize(bin_size - 1, 0x00);
					num.push(sign_byte);
				}

				stack.push(num);
			},
			Opcode::OP_CAT | Opcode::OP_SUBSTR | Opcode::OP_LEFT | Opcode::OP_RIGHT |
			Opcode::OP_INVERT | Opcode::OP_AND | Opcode::OP_OR | Opcode::OP_XOR |
			Opcode::OP_2MUL | Opcode::OP_2DIV | Opcode::OP_MUL | Opcode::OP_DIV |
			Opcode::OP_MOD | Opcode::OP_LSHIFT | Opcode::OP_RSHIFT => {
				return Err(Error::DisabledOpcode(opcode));
			},
			Opcode::OP_NOP => (),
			Opcode::OP_CHECKLOCKTIMEVERIFY => {
				if flags.verify_locktime {
					// Note that elsewhere numeric opcodes are limited to
					// operands in the range -2**31+1 to 2**31-1, however it is
					// legal for opcodes to produce results exceeding that
					// range. This limitation is implemented by CScriptNum's
					// default 4-byte limit.
					//
					// If we kept to that limit we'd have a year 2038 problem,
					// even though the nLockTime field in transactions
					// themselves is uint32 which only becomes meaningless
					// after the year 2106.
					//
					// Thus as a special case we tell CScriptNum to accept up
					// to 5-byte bignums, which are good until 2**39-1, well
					// beyond the 2**32-1 limit of the nLockTime field itself.
					let lock_time = Num::from_slice(stack.last()?, flags.verify_minimaldata, 5)?;

					// In the rare event that the argument may be < 0 due to
					// some arithmetic being done first, you can always use
					// 0 MAX CHECKLOCKTIMEVERIFY.
					if lock_time.is_negative() {
						return Err(Error::NegativeLocktime);
					}

					if !checker.check_lock_time(lock_time) {
						return Err(Error::UnsatisfiedLocktime);
					}
				} else if flags.verify_discourage_upgradable_nops {
					return Err(Error::DiscourageUpgradableNops);
				}
			},
			Opcode::OP_CHECKSEQUENCEVERIFY => {
				if flags.verify_checksequence {
					let sequence = Num::from_slice(stack.last()?, flags.verify_minimaldata, 5)?;

					if sequence.is_negative() {
						return Err(Error::NegativeLocktime);
					}

					if (sequence & (SEQUENCE_LOCKTIME_DISABLE_FLAG as i64).into()).is_zero() && !checker.check_sequence(sequence) {
						return Err(Error::UnsatisfiedLocktime);
					}

				} else if flags.verify_discourage_upgradable_nops {
					return Err(Error::DiscourageUpgradableNops);
				}
			},
			Opcode::OP_NOP1 |
			Opcode::OP_NOP4 |
			Opcode::OP_NOP5 |
			Opcode::OP_NOP6 |
			Opcode::OP_NOP7 |
			Opcode::OP_NOP8 |
			Opcode::OP_NOP9 |
			Opcode::OP_NOP10 => {
				if flags.verify_discourage_upgradable_nops {
					return Err(Error::DiscourageUpgradableNops);
				}
			},
			Opcode::OP_IF | Opcode::OP_NOTIF => {
				let mut exec_value = false;
				if executing {
					exec_value = cast_to_bool(&stack.pop().map_err(|_| Error::UnbalancedConditional)?);
					if opcode == Opcode::OP_NOTIF {
						exec_value = !exec_value;
					}
				}
				exec_stack.push(exec_value);
			},
			Opcode::OP_ELSE => {
				if exec_stack.is_empty() {
					return Err(Error::UnbalancedConditional);
				}
				let last_index = exec_stack.len() - 1;
				let last = exec_stack[last_index];
				exec_stack[last_index] = !last;
			},
			Opcode::OP_ENDIF => {
				if exec_stack.is_empty() {
					return Err(Error::UnbalancedConditional);
				}
				exec_stack.pop();
			},
			Opcode::OP_VERIFY => {
				let exec_value = cast_to_bool(&stack.pop()?);
				if !exec_value {
					return Err(Error::Verify);
				}
			},
			Opcode::OP_RETURN => {
				return Err(Error::ReturnOpcode);
			},
			Opcode::OP_TOALTSTACK => {
				altstack.push(stack.pop()?);
			},
			Opcode::OP_FROMALTSTACK => {
				stack.push(altstack.pop().map_err(|_| Error::InvalidAltstackOperation)?);
			},
			Opcode::OP_2DROP => {
				stack.drop(2)?;
			},
			Opcode::OP_2DUP => {
				stack.dup(2)?;
			},
			Opcode::OP_3DUP => {
				stack.dup(3)?;
			},
			Opcode::OP_2OVER => {
				stack.over(2)?;
			},
			Opcode::OP_2ROT => {
				stack.rot(2)?;
			},
			Opcode::OP_2SWAP => {
				stack.swap(2)?;
			},
			Opcode::OP_IFDUP => {
				if cast_to_bool(stack.last()?) {
					stack.dup(1)?;
				}
			},
			Opcode::OP_DEPTH => {
				let depth = Num::from(stack.len());
				stack.push(depth.to_bytes());
			},
			Opcode::OP_DROP => {
				stack.pop()?;
			},
			Opcode::OP_DUP => {
				stack.dup(1)?;
			},
			Opcode::OP_NIP => {
				stack.nip()?;
			},
			Opcode::OP_OVER => {
				stack.over(1)?;
			},
			Opcode::OP_PICK | Opcode::OP_ROLL => {
				let n: i64 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?.into();
				if n < 0 || n >= stack.len() as i64 {
					return Err(Error::InvalidStackOperation);
				}

				let v = match opcode {
					Opcode::OP_PICK => stack.top(n as usize)?.clone(),
					_ => stack.remove(n as usize)?,
				};

				stack.push(v);
			},
			Opcode::OP_ROT => {
				stack.rot(1)?;
			},
			Opcode::OP_SWAP => {
				stack.swap(1)?;
			},
			Opcode::OP_TUCK => {
				stack.tuck()?;
			},
			Opcode::OP_SIZE => {
				let n = Num::from(stack.last()?.len());
				stack.push(n.to_bytes());
			},
			Opcode::OP_EQUAL => {
				let v1 = stack.pop()?;
				let v2 = stack.pop()?;
				if v1 == v2 {
					stack.push(vec![1].into());
				} else {
					stack.push(Bytes::new());
				}
			},
			Opcode::OP_EQUALVERIFY => {
				let equal = stack.pop()? == stack.pop()?;
				if !equal {
					return Err(Error::EqualVerify);
				}
			},
			Opcode::OP_1ADD => {
				let n = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)? + 1.into();
				stack.push(n.to_bytes());
			},
			Opcode::OP_1SUB => {
				let n = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)? - 1.into();
				stack.push(n.to_bytes());
			},
			Opcode::OP_NEGATE => {
				let n = -Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				stack.push(n.to_bytes());
			},
			Opcode::OP_ABS => {
				let n = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?.abs();
				stack.push(n.to_bytes());
			},
			Opcode::OP_NOT => {
				let n = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?.is_zero();
				let n = Num::from(n);
				stack.push(n.to_bytes());
			},
			Opcode::OP_0NOTEQUAL => {
				let n = !Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?.is_zero();
				let n = Num::from(n);
				stack.push(n.to_bytes());
			},
			Opcode::OP_ADD => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				stack.push((v1 + v2).to_bytes());
			},
			Opcode::OP_SUB => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				stack.push((v2 - v1).to_bytes());
			},
			Opcode::OP_BOOLAND => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(!v1.is_zero() && !v2.is_zero());
				stack.push(v.to_bytes());
			},
			Opcode::OP_BOOLOR => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(!v1.is_zero() || !v2.is_zero());
				stack.push(v.to_bytes());
			},
			Opcode::OP_NUMEQUAL => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 == v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_NUMEQUALVERIFY => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if v1 != v2 {
					return Err(Error::NumEqualVerify);
				}
			},
			Opcode::OP_NUMNOTEQUAL => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 != v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_LESSTHAN => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 > v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_GREATERTHAN => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 < v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_LESSTHANOREQUAL => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 >= v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_GREATERTHANOREQUAL => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v = Num::from(v1 <= v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_MIN => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				stack.push(cmp::min(v1, v2).to_bytes());
			},
			Opcode::OP_MAX => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				stack.push(cmp::max(v1, v2).to_bytes());
			},
			Opcode::OP_WITHIN => {
				let v1 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v2 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				let v3 = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if v2 <= v3 && v3 < v1 {
					stack.push(vec![1].into());
				} else {
					stack.push(Bytes::new());
				}
			},
			Opcode::OP_RIPEMD160 => {
				let v = ripemd160(&stack.pop()?);
				stack.push(v.to_vec().into());
			},
			Opcode::OP_SHA1 => {
				let v = sha1(&stack.pop()?);
				stack.push(v.to_vec().into());
			},
			Opcode::OP_SHA256 => {
				let v = sha256(&stack.pop()?);
				stack.push(v.to_vec().into());
			},
			Opcode::OP_HASH160 => {
				let v = dhash160(&stack.pop()?);
				stack.push(v.to_vec().into());
			},
			Opcode::OP_HASH256 => {
				let v = dhash256(&stack.pop()?);
				stack.push(v.to_vec().into());
			},
			Opcode::OP_CODESEPARATOR => {
				begincode = pc;
			},
			Opcode::OP_CHECKSIG | Opcode::OP_CHECKSIGVERIFY => {
				let pubkey = stack.pop()?;
				let signature = stack.pop()?;
				let sighash = parse_hash_type(version, &signature);
				let mut subscript = script.subscript(begincode);
				match version {
					SignatureVersion::ForkId if sighash.fork_id => (),
					SignatureVersion::WitnessV0 => (),
					SignatureVersion::Base | SignatureVersion::ForkId => {
						let signature_script = Builder::default().push_data(&*signature).into_script();
						subscript = subscript.find_and_delete(&*signature_script);
					},
				}

				check_signature_encoding(&signature, flags, version)?;
				check_pubkey_encoding(&pubkey, flags)?;

				let success = check_signature(checker, signature.into(), pubkey.into(), &subscript, version);
				match opcode {
					Opcode::OP_CHECKSIG => {
						if success {
							stack.push(vec![1].into());
						} else {
							stack.push(Bytes::new());
						}
					},
					Opcode::OP_CHECKSIGVERIFY if !success => {
						return Err(Error::CheckSigVerify);
					},
					_ => {},
				}
			},
			Opcode::OP_CHECKMULTISIG | Opcode::OP_CHECKMULTISIGVERIFY => {
				let keys_count = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if keys_count < 0.into() || keys_count > script::MAX_PUBKEYS_PER_MULTISIG.into() {
					return Err(Error::PubkeyCount);
				}

				let keys_count: usize = keys_count.into();
				let keys = (0..keys_count).into_iter().map(|_| stack.pop()).collect::<Result<Vec<_>, _>>()?;

				let sigs_count = Num::from_slice(&stack.pop()?, flags.verify_minimaldata, 4)?;
				if sigs_count < 0.into() || sigs_count > keys_count.into() {
					return Err(Error::SigCount);
				}

				let sigs_count: usize = sigs_count.into();
				let sigs = (0..sigs_count).into_iter().map(|_| stack.pop()).collect::<Result<Vec<_>, _>>()?;

				let mut subscript = script.subscript(begincode);

				for signature in &sigs {
					let sighash = parse_hash_type(version, &signature);
					match version {
						SignatureVersion::ForkId if sighash.fork_id => (),
						SignatureVersion::WitnessV0 => (),
						SignatureVersion::Base | SignatureVersion::ForkId => {
							let signature_script = Builder::default().push_data(&*signature).into_script();
							subscript = subscript.find_and_delete(&*signature_script);
						},
					}
				}

				let mut success = true;
				let mut k = 0;
				let mut s = 0;
				while s < sigs.len() && success {
					// TODO: remove redundant copying
					let key = keys[k].clone();
					let sig = sigs[s].clone();

					check_signature_encoding(&sig, flags, version)?;
					check_pubkey_encoding(&key, flags)?;

					let ok = check_signature(checker, sig.into(), key.into(), &subscript, version);
					if ok {
						s += 1;
					}
					k += 1;

					success = sigs.len() - s <= keys.len() - k;
				}

				if !stack.pop()?.is_empty() && flags.verify_nulldummy {
					return Err(Error::SignatureNullDummy);
				}

				match opcode {
					Opcode::OP_CHECKMULTISIG => {
						if success {
							stack.push(vec![1].into());
						} else {
							stack.push(Bytes::new());
						}
					},
					Opcode::OP_CHECKMULTISIGVERIFY if !success => {
						return Err(Error::CheckSigVerify);
					},
					_ => {},
				}
			},
			Opcode::OP_RESERVED |
			Opcode::OP_VER |
			Opcode::OP_RESERVED1 |
			Opcode::OP_RESERVED2 => {
				if executing {
					return Err(Error::DisabledOpcode(opcode));
				}
			},
			Opcode::OP_VERIF |
			Opcode::OP_VERNOTIF => {
				return Err(Error::DisabledOpcode(opcode));
			},
			Opcode::OP_CHECKDATASIG | Opcode::OP_CHECKDATASIGVERIFY if flags.verify_checkdatasig => {
				let pubkey = stack.pop()?;
				let message = stack.pop()?;
				let signature = stack.pop()?;

				check_signature_encoding(&signature, flags, version)?;
				check_pubkey_encoding(&pubkey, flags)?;

				let signature: Vec<u8> = signature.into();
				let message_hash = sha256(&message);
				let success = verify_signature(checker, signature.into(), pubkey.into(), message_hash);
				match opcode {
					Opcode::OP_CHECKDATASIG => {
						if success {
							stack.push(vec![1].into());
						} else {
							stack.push(Bytes::new());
						}
					},
					Opcode::OP_CHECKDATASIGVERIFY if !success => {
						return Err(Error::CheckDataSigVerify);
					},
					_ => {},
				}
			},
			Opcode::OP_CHECKDATASIG | Opcode::OP_CHECKDATASIGVERIFY => {
				return Err(Error::DisabledOpcode(opcode));
			},
		}

		if stack.len() + altstack.len() > 1000 {
			return Err(Error::StackSize);
		}
	}

	if !exec_stack.is_empty() {
		return Err(Error::UnbalancedConditional);
	}

	let success = !stack.is_empty() && {
		let last = stack.last()?;
		cast_to_bool(last)
	};

	Ok(success)
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use chain::Transaction;
	use crypto::sha256;
	use keys::{KeyPair, Private, Message, Network};
	use sign::SignatureVersion;
	use script::MAX_SCRIPT_ELEMENT_SIZE;
	use {
		Opcode, Script, ScriptWitness, VerificationFlags, Builder, Error, Num, TransactionInputSigner,
		NoopSignatureChecker, TransactionSignatureChecker, Stack
	};
	use super::{eval_script, verify_script, is_public_key};

	#[test]
	fn tests_is_public_key() {
		assert!(!is_public_key(&[]));
		assert!(!is_public_key(&[1]));
		assert!(is_public_key(&Bytes::from("0495dfb90f202c7d016ef42c65bc010cd26bb8237b06253cc4d12175097bef767ed6b1fcb3caf1ed57c98d92e6cb70278721b952e29a335134857acd4c199b9d2f")));
		assert!(is_public_key(&[2; 33]));
		assert!(is_public_key(&[3; 33]));
		assert!(!is_public_key(&[4; 33]));
	}

	// https://github.com/bitcoin/bitcoin/blob/d612837814020ae832499d18e6ee5eb919a87907/src/test/script_tests.cpp#L900
	#[test]
	fn test_push_data() {
		let expected: Stack<Bytes> = vec![vec![0x5a].into()].into();
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		let checker = NoopSignatureChecker;
		let version = SignatureVersion::Base;
		let direct: Script = vec![Opcode::OP_PUSHBYTES_1 as u8, 0x5a].into();
		let pushdata1: Script = vec![Opcode::OP_PUSHDATA1 as u8, 0x1, 0x5a].into();
		let pushdata2: Script = vec![Opcode::OP_PUSHDATA2 as u8, 0x1, 0, 0x5a].into();
		let pushdata4: Script = vec![Opcode::OP_PUSHDATA4 as u8, 0x1, 0, 0, 0, 0x5a].into();

		let mut direct_stack = Stack::new();
		let mut pushdata1_stack = Stack::new();
		let mut pushdata2_stack = Stack::new();
		let mut pushdata4_stack = Stack::new();
		assert!(eval_script(&mut direct_stack, &direct, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata1_stack, &pushdata1, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata2_stack, &pushdata2, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata4_stack, &pushdata4, &flags, &checker, version).unwrap());

		assert_eq!(direct_stack, expected);
		assert_eq!(pushdata1_stack, expected);
		assert_eq!(pushdata2_stack, expected);
		assert_eq!(pushdata4_stack, expected);
	}

	fn basic_test_with_flags(script: &Script, flags: &VerificationFlags, expected: Result<bool, Error>, expected_stack: Stack<Bytes>) {
		let checker = NoopSignatureChecker;
		let version = SignatureVersion::Base;
		let mut stack = Stack::new();
		assert_eq!(eval_script(&mut stack, script, &flags, &checker, version), expected);
		if expected.is_ok() {
			assert_eq!(stack, expected_stack);
		}
	}

	fn basic_test(script: &Script, expected: Result<bool, Error>, expected_stack: Stack<Bytes>) {
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		basic_test_with_flags(script, &flags, expected, expected_stack)
	}

	#[test]
	fn test_equal() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![vec![1].into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_equal_false() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_data(&[0x3])
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Bytes::new()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_equal_invalid_stack() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_equal_verify() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUALVERIFY)
			.into_script();
		let result = Ok(false);
		let stack = Stack::default();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_equal_verify_failed() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_data(&[0x3])
			.push_opcode(Opcode::OP_EQUALVERIFY)
			.into_script();
		let result = Err(Error::EqualVerify);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_equal_verify_invalid_stack() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUALVERIFY)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_size() {
		let script = Builder::default()
			.push_data(&[0x12, 0x34])
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Ok(true);
		let stack = vec![vec![0x12, 0x34].into(), vec![0x2].into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_size_false() {
		let script = Builder::default()
			.push_data(&[])
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Ok(false);
		let stack = vec![vec![].into(), vec![].into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_size_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_hash256() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_HASH256)
			.into_script();
		let result = Ok(true);
		let stack = vec!["9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_hash256_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_HASH256)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_ripemd160() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_RIPEMD160)
			.into_script();
		let result = Ok(true);
		let stack = vec!["108f07b8382412612c048d07d13f814118445acd".into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_ripemd160_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_RIPEMD160)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_sha1() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_SHA1)
			.into_script();
		let result = Ok(true);
		let stack = vec!["aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sha1_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SHA1)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_sha256() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_SHA256)
			.into_script();
		let result = Ok(true);
		let stack = vec!["2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sha256_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SHA256)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_1add() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1ADD)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(6).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_1add_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_1ADD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_1sub() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1SUB)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(4).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_1sub_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_1SUB)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_negate() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(-5).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_negate_negative() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_negate_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_abs() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_abs_negative() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_abs_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_not() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_not_zero() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_not_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_0notequal() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_0notequal_zero() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_0notequal_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_add() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_add_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_sub() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUB)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sub_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUB)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_booland() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_booland_first() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_booland_second() {
		let script = Builder::default()
			.push_num(0.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_booland_none() {
		let script = Builder::default()
			.push_num(0.into())
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_booland_invalid_stack() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_boolor() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_boolor_first() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_boolor_second() {
		let script = Builder::default()
			.push_num(0.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_boolor_none() {
		let script = Builder::default()
			.push_num(0.into())
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_boolor_invalid_stack() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_numequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numequal_not() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_NUMEQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_numequalverify() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Ok(false);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_numequalverify_failed() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Err(Error::NumEqualVerify);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_numequalverify_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_numnotequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_NUMNOTEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numnotequal_not() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMNOTEQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numnotequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMNOTEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_lessthan() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_LESSTHAN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthan_not() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHAN)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthan_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHAN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_greaterthan() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHAN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthan_not() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHAN)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthan_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHAN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_lessthanorequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthanorequal_equal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthanorequal_not() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(1.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthanorequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_greaterthanorequal() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthanorequal_equal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthanorequal_not() {
		let script = Builder::default()
			.push_num(1.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthanorequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_min() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MIN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(2).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_min_second() {
		let script = Builder::default()
			.push_num(4.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MIN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(3).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_min_invalid_stack() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_MIN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_max() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MAX)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(3).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_max_second() {
		let script = Builder::default()
			.push_num(4.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MAX)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(4).to_bytes()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_max_invalid_stack() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_MAX)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_within() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_num(4.into())
			.push_opcode(Opcode::OP_WITHIN)
			.into_script();
		let result = Ok(true);
		let stack = vec![vec![1].into()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_within_not() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(5.into())
			.push_num(4.into())
			.push_opcode(Opcode::OP_WITHIN)
			.into_script();
		let result = Ok(false);
		let stack = vec![Bytes::new()].into();
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_within_invalid_stack() {
		let script = Builder::default()
			.push_num(5.into())
			.push_num(4.into())
			.push_opcode(Opcode::OP_WITHIN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, Stack::default());
	}

	#[test]
	fn test_within_testnet_block_519() {
		let script = Builder::default()
			.push_num(1.into())
			.push_num(0.into())
			.push_num(1.into())
			.push_opcode(Opcode::OP_WITHIN)
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Ok(true);
		let stack = vec![vec![1].into()].into();
		basic_test(&script, result, stack);
	}

	// https://blockchain.info/rawtx/3f285f083de7c0acabd9f106a43ec42687ab0bebe2e6f0d529db696794540fea
	#[test]
	fn test_check_transaction_signature() {
		let tx: Transaction = "0100000001484d40d45b9ea0d652fca8258ab7caa42541eb52975857f96fb50cd732c8b481000000008a47304402202cb265bf10707bf49346c3515dd3d16fc454618c58ec0a0ff448a676c54ff71302206c6624d762a1fcef4618284ead8f08678ac05b13c84235f1654e6ad168233e8201410414e301b2328f17442c0b8310d787bf3d8a404cfbd0704f135b6ad4b2d3ee751310f981926e53a6e8c39bd7d3fefd576c543cce493cbac06388f2651d1aacbfcdffffffff0162640100000000001976a914c8e90996c7c6080ee06284600c684ed904d14c5c88ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};
		let input: Script = "47304402202cb265bf10707bf49346c3515dd3d16fc454618c58ec0a0ff448a676c54ff71302206c6624d762a1fcef4618284ead8f08678ac05b13c84235f1654e6ad168233e8201410414e301b2328f17442c0b8310d787bf3d8a404cfbd0704f135b6ad4b2d3ee751310f981926e53a6e8c39bd7d3fefd576c543cce493cbac06388f2651d1aacbfcd".into();
		let output: Script = "76a914df3bd30160e6c6145baaf2c88a8844c13a00d1d588ac".into();
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}

	// https://blockchain.info/rawtx/02b082113e35d5386285094c2829e7e2963fa0b5369fb7f4b79c4c90877dcd3d
	#[test]
	fn test_check_transaction_multisig() {
		let tx: Transaction = "01000000013dcd7d87904c9cb7f4b79f36b5a03f96e2e729284c09856238d5353e1182b00200000000fd5e0100483045022100deeb1f13b5927b5e32d877f3c42a4b028e2e0ce5010fdb4e7f7b5e2921c1dcd2022068631cb285e8c1be9f061d2968a18c3163b780656f30a049effee640e80d9bff01483045022100ee80e164622c64507d243bd949217d666d8b16486e153ac6a1f8e04c351b71a502203691bef46236ca2b4f5e60a82a853a33d6712d6a1e7bf9a65e575aeb7328db8c014cc9524104a882d414e478039cd5b52a92ffb13dd5e6bd4515497439dffd691a0f12af9575fa349b5694ed3155b136f09e63975a1700c9f4d4df849323dac06cf3bd6458cd41046ce31db9bdd543e72fe3039a1f1c047dab87037c36a669ff90e28da1848f640de68c2fe913d363a51154a0c62d7adea1b822d05035077418267b1a1379790187410411ffd36c70776538d079fbae117dc38effafb33304af83ce4894589747aee1ef992f63280567f52f5ba870678b4ab4ff6c8ea600bd217870a8b4f1f09f3a8e8353aeffffffff0130d90000000000001976a914569076ba39fc4ff6a2291d9ea9196d8c08f9c7ab88ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};
		let input: Script = "00483045022100deeb1f13b5927b5e32d877f3c42a4b028e2e0ce5010fdb4e7f7b5e2921c1dcd2022068631cb285e8c1be9f061d2968a18c3163b780656f30a049effee640e80d9bff01483045022100ee80e164622c64507d243bd949217d666d8b16486e153ac6a1f8e04c351b71a502203691bef46236ca2b4f5e60a82a853a33d6712d6a1e7bf9a65e575aeb7328db8c014cc9524104a882d414e478039cd5b52a92ffb13dd5e6bd4515497439dffd691a0f12af9575fa349b5694ed3155b136f09e63975a1700c9f4d4df849323dac06cf3bd6458cd41046ce31db9bdd543e72fe3039a1f1c047dab87037c36a669ff90e28da1848f640de68c2fe913d363a51154a0c62d7adea1b822d05035077418267b1a1379790187410411ffd36c70776538d079fbae117dc38effafb33304af83ce4894589747aee1ef992f63280567f52f5ba870678b4ab4ff6c8ea600bd217870a8b4f1f09f3a8e8353ae".into();
		let output: Script = "a9141a8b0026343166625c7475f01e48b5ede8c0252e87".into();
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}

	// https://blockchain.info/en/tx/12b5633bad1f9c167d523ad1aa1947b2732a865bf5414eab2f9e5ae5d5c191ba?show_adv=true
	#[test]
	fn test_transaction_with_high_s_signature() {
		let tx: Transaction = "010000000173805864da01f15093f7837607ab8be7c3705e29a9d4a12c9116d709f8911e590100000049483045022052ffc1929a2d8bd365c6a2a4e3421711b4b1e1b8781698ca9075807b4227abcb0221009984107ddb9e3813782b095d0d84361ed4c76e5edaf6561d252ae162c2341cfb01ffffffff0200e1f50500000000434104baa9d36653155627c740b3409a734d4eaf5dcca9fb4f736622ee18efcf0aec2b758b2ec40db18fbae708f691edb2d4a2a3775eb413d16e2e3c0f8d4c69119fd1ac009ce4a60000000043410411db93e1dcdb8a016b49840f8c53bc1eb68a382e97b1482ecad7b148a6909a5cb2e0eaddfb84ccf9744464f82e160bfa9b8b64f9d4c03f999b8643f656b412a3ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};
		let input: Script = "483045022052ffc1929a2d8bd365c6a2a4e3421711b4b1e1b8781698ca9075807b4227abcb0221009984107ddb9e3813782b095d0d84361ed4c76e5edaf6561d252ae162c2341cfb01".into();
		let output: Script = "410411db93e1dcdb8a016b49840f8c53bc1eb68a382e97b1482ecad7b148a6909a5cb2e0eaddfb84ccf9744464f82e160bfa9b8b64f9d4c03f999b8643f656b412a3ac".into();
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}

	// https://blockchain.info/rawtx/fb0a1d8d34fa5537e461ac384bac761125e1bfa7fec286fa72511240fa66864d
	#[test]
	fn test_transaction_from_124276() {
		let tx: Transaction = "01000000012316aac445c13ff31af5f3d1e2cebcada83e54ba10d15e01f49ec28bddc285aa000000008e4b3048022200002b83d59c1d23c08efd82ee0662fec23309c3adbcbd1f0b8695378db4b14e736602220000334a96676e58b1bb01784cb7c556dd8ce1c220171904da22e18fe1e7d1510db5014104d0fe07ff74c9ef5b00fed1104fad43ecf72dbab9e60733e4f56eacf24b20cf3b8cd945bcabcc73ba0158bf9ce769d43e94bd58c5c7e331a188922b3fe9ca1f5affffffff01c0c62d00000000001976a9147a2a3b481ca80c4ba7939c54d9278e50189d94f988ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};
		let input: Script = "4b3048022200002b83d59c1d23c08efd82ee0662fec23309c3adbcbd1f0b8695378db4b14e736602220000334a96676e58b1bb01784cb7c556dd8ce1c220171904da22e18fe1e7d1510db5014104d0fe07ff74c9ef5b00fed1104fad43ecf72dbab9e60733e4f56eacf24b20cf3b8cd945bcabcc73ba0158bf9ce769d43e94bd58c5c7e331a188922b3fe9ca1f5a".into();
		let output: Script = "76a9147a2a3b481ca80c4ba7939c54d9278e50189d94f988ac".into();
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}

	// https://blockchain.info/rawtx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
	#[test]
	fn test_transaction_bip65() {
		let tx: Transaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 1,
			input_amount: 0,
		};
		let input: Script = "004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51ae".into();
		let output: Script = "142a9bc5447d664c1d0141392a842d23dba45c4f13b175".into();

		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));

		let flags = VerificationFlags::default()
			.verify_p2sh(true)
			.verify_locktime(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Err(Error::NumberOverflow));
	}

	// https://blockchain.info/rawtx/54fabd73f1d20c980a0686bf0035078e07f69c58437e4d586fb29aa0bee9814f
	#[test]
	fn test_arithmetic_correct_arguments_order() {
		let tx: Transaction = "01000000010c0e314bd7bb14721b3cfd8e487cd6866173354f87ca2cf4d13c8d3feb4301a6000000004a483045022100d92e4b61452d91a473a43cde4b469a472467c0ba0cbd5ebba0834e4f4762810402204802b76b7783db57ac1f61d2992799810e173e91055938750815b6d8a675902e014fffffffff0140548900000000001976a914a86e8ee2a05a44613904e18132e49b2448adc4e688ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 0,
		};
		let input: Script = "483045022100d92e4b61452d91a473a43cde4b469a472467c0ba0cbd5ebba0834e4f4762810402204802b76b7783db57ac1f61d2992799810e173e91055938750815b6d8a675902e014f".into();
		let output: Script = "76009f69905160a56b210378d430274f8c5ec1321338151e9f27f4c676a008bdf8638d07c0b6be9ab35c71ad6c".into();
		let flags = VerificationFlags::default();
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}

	#[test]
	fn test_invalid_opcode_in_dead_execution_path_b83() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_0)
			.push_opcode(Opcode::OP_IF)
			.push_invalid_opcode()
			.push_opcode(Opcode::OP_ELSE)
			.push_opcode(Opcode::OP_1)
			.push_opcode(Opcode::OP_ENDIF)
			.into_script();
		let result = Ok(true);
		basic_test(&script, result, vec![vec![1].into()].into());
	}

	#[test]
	fn test_skipping_sequencetimeverify() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_1)
			.push_opcode(Opcode::OP_NOP1)
			.push_opcode(Opcode::OP_CHECKLOCKTIMEVERIFY)
			.push_opcode(Opcode::OP_CHECKSEQUENCEVERIFY)
			.push_opcode(Opcode::OP_NOP4)
			.push_opcode(Opcode::OP_NOP5)
			.push_opcode(Opcode::OP_NOP6)
			.push_opcode(Opcode::OP_NOP7)
			.push_opcode(Opcode::OP_NOP8)
			.push_opcode(Opcode::OP_NOP9)
			.push_opcode(Opcode::OP_NOP10)
			.push_opcode(Opcode::OP_1)
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();
		let result = Ok(true);
		basic_test(&script, result, vec![vec![1].into()].into());
	}

	// https://webbtc.com/tx/5df1375ffe61ac35ca178ebb0cab9ea26dedbd0e96005dfcee7e379fa513232f
	#[test]
	fn test_transaction_find_and_delete() {
		let tx: Transaction = "0100000002f9cbafc519425637ba4227f8d0a0b7160b4e65168193d5af39747891de98b5b5000000006b4830450221008dd619c563e527c47d9bd53534a770b102e40faa87f61433580e04e271ef2f960220029886434e18122b53d5decd25f1f4acb2480659fea20aabd856987ba3c3907e0121022b78b756e2258af13779c1a1f37ea6800259716ca4b7f0b87610e0bf3ab52a01ffffffff42e7988254800876b69f24676b3e0205b77be476512ca4d970707dd5c60598ab00000000fd260100483045022015bd0139bcccf990a6af6ec5c1c52ed8222e03a0d51c334df139968525d2fcd20221009f9efe325476eb64c3958e4713e9eefe49bf1d820ed58d2112721b134e2a1a53034930460221008431bdfa72bc67f9d41fe72e94c88fb8f359ffa30b33c72c121c5a877d922e1002210089ef5fc22dd8bfc6bf9ffdb01a9862d27687d424d1fefbab9e9c7176844a187a014c9052483045022015bd0139bcccf990a6af6ec5c1c52ed8222e03a0d51c334df139968525d2fcd20221009f9efe325476eb64c3958e4713e9eefe49bf1d820ed58d2112721b134e2a1a5303210378d430274f8c5ec1321338151e9f27f4c676a008bdf8638d07c0b6be9ab35c71210378d430274f8c5ec1321338151e9f27f4c676a008bdf8638d07c0b6be9ab35c7153aeffffffff01a08601000000000017a914d8dacdadb7462ae15cd906f1878706d0da8660e68700000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 1,
			input_amount: 0,
		};
		let input: Script = "00483045022015BD0139BCCCF990A6AF6EC5C1C52ED8222E03A0D51C334DF139968525D2FCD20221009F9EFE325476EB64C3958E4713E9EEFE49BF1D820ED58D2112721B134E2A1A53034930460221008431BDFA72BC67F9D41FE72E94C88FB8F359FFA30B33C72C121C5A877D922E1002210089EF5FC22DD8BFC6BF9FFDB01A9862D27687D424D1FEFBAB9E9C7176844A187A014C9052483045022015BD0139BCCCF990A6AF6EC5C1C52ED8222E03A0D51C334DF139968525D2FCD20221009F9EFE325476EB64C3958E4713E9EEFE49BF1D820ED58D2112721B134E2A1A5303210378D430274F8C5EC1321338151E9F27F4C676A008BDF8638D07C0B6BE9AB35C71210378D430274F8C5EC1321338151E9F27F4C676A008BDF8638D07C0B6BE9AB35C7153AE".into();
		let output: Script = "A914D8DACDADB7462AE15CD906F1878706D0DA8660E687".into();

		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}


	#[test]
	fn test_script_with_forkid_signature() {
		use sign::UnsignedTransactionInput;
		use chain::{OutPoint, TransactionOutput};

		let key_pair = KeyPair::from_private(Private { network: Network::Mainnet, secret: 1.into(), compressed: false, }).unwrap();
		let redeem_script = Builder::default()
			.push_data(key_pair.public())
			.push_opcode(Opcode::OP_CHECKSIG)
			.into_script();


		let amount = 12345000000000;
		let sighashtype = 0x41; // All + ForkId
		let checker = TransactionSignatureChecker {
			input_index: 0,
			input_amount: amount,
			signer: TransactionInputSigner {
				version: 1,
				inputs: vec![
					UnsignedTransactionInput {
						previous_output: OutPoint {
							hash: 0u8.into(),
							index: 0xffffffff,
						},
						sequence: 0xffffffff,
					},
				],
				outputs: vec![
					TransactionOutput {
						value: amount,
						script_pubkey: redeem_script.to_bytes(),
					},
				],
				lock_time: 0,
			},
		};

		let script_pubkey = redeem_script;
		let flags = VerificationFlags::default();

		// valid signature
		{
			let signed_input = checker.signer.signed_input(&key_pair, 0, amount, &script_pubkey, SignatureVersion::ForkId, sighashtype);
			let script_sig = signed_input.script_sig.into();

			assert_eq!(verify_script(&script_sig, &script_pubkey, &ScriptWitness::default(), &flags, &checker, SignatureVersion::ForkId), Ok(()));
		}

		// signature with wrong amount
		{
			let signed_input = checker.signer.signed_input(&key_pair, 0, amount + 1, &script_pubkey, SignatureVersion::ForkId, sighashtype);
			let script_sig = signed_input.script_sig.into();

			assert_eq!(verify_script(&script_sig, &script_pubkey, &ScriptWitness::default(), &flags, &checker, SignatureVersion::ForkId), Err(Error::EvalFalse));
		}

		// fork-id signature passed when not expected
		{
			let signed_input = checker.signer.signed_input(&key_pair, 0, amount + 1, &script_pubkey, SignatureVersion::ForkId, sighashtype);
			let script_sig = signed_input.script_sig.into();

			assert_eq!(verify_script(&script_sig, &script_pubkey, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Err(Error::EvalFalse));
		}

		// non-fork-id signature passed when expected
		{
			let signed_input = checker.signer.signed_input(&key_pair, 0, amount + 1, &script_pubkey, SignatureVersion::Base, 1);
			let script_sig = signed_input.script_sig.into();

			assert_eq!(verify_script(&script_sig, &script_pubkey, &ScriptWitness::default(), &flags.verify_strictenc(true), &checker, SignatureVersion::ForkId), Err(Error::SignatureMustUseForkId));
		}
	}

	fn run_witness_test(script_sig: Script, script_pubkey: Script, script_witness: Vec<Bytes>, flags: VerificationFlags, amount: u64) -> Result<(), Error> {
		use chain::{TransactionInput, OutPoint, TransactionOutput};

		let tx1 = Transaction {
			version: 1,
			inputs: vec![TransactionInput {
				previous_output: OutPoint {
					hash: Default::default(),
					index: 0xffffffff,
				},
				script_sig: Builder::default().push_num(0.into()).push_num(0.into()).into_bytes(),
				sequence: 0xffffffff,
				script_witness: vec![],
			}],
			outputs: vec![TransactionOutput {
				value: amount,
				script_pubkey: script_pubkey.to_bytes(),
			}],
			lock_time: 0,
		};
		let tx2 = Transaction {
			version: 1,
			inputs: vec![TransactionInput {
				previous_output: OutPoint {
					hash: tx1.hash(),
					index: 0,
				},
				script_sig: script_sig.to_bytes(),
				sequence: 0xffffffff,
				script_witness: script_witness.clone(),
			}],
			outputs: vec![TransactionOutput {
				value: amount,
				script_pubkey: Builder::default().into_bytes(),
			}],
			lock_time: 0,
		};

		let checker = TransactionSignatureChecker {
			input_index: 0,
			input_amount: amount,
			signer: tx2.into(),
		};

		verify_script(&script_sig,
			&script_pubkey,
			&script_witness,
			&flags,
			&checker,
			SignatureVersion::Base)
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1257
	#[test]
	fn witness_invalid_script() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("".into(),
				"00206e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d".into(),
				vec!["00".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1258
	#[test]
	fn witness_script_hash_mismatch() {
		assert_eq!(Err(Error::WitnessProgramMismatch),
			run_witness_test("".into(),
				"00206e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d".into(),
				vec!["51".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1259
	#[test]
	fn witness_invalid_script_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"00206e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d".into(),
				vec!["00".into()],
				VerificationFlags::default(),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1260
	#[test]
	fn witness_script_hash_mismatch_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"00206e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d".into(),
				vec!["51".into()],
				VerificationFlags::default(),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1860
	#[test]
	fn witness_basic_p2wsh() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"0020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				vec!["304402200d461c140cfdfcf36b94961db57ae8c18d1cb80e9d95a9e47ac22470c1bf125502201c8dc1cbfef6a3ef90acbbb992ca22fe9466ee6f9d4898eda277a7ac3ab4b25101".into(),
					"410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1872
	#[test]
	fn witness_basic_p2wpkh() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				vec!["304402201e7216e5ccb3b61d46946ec6cc7e8c4e0117d13ac2fd4b152197e4805191c74202203e9903e33e84d9ee1dd13fb057afb7ccfb47006c23f6a067185efbc9dd780fc501".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1884
	#[test]
	fn witness_basic_p2sh_p2wsh() {
		assert_eq!(Ok(()),
			run_witness_test("220020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				"a914f386c2ba255cc56d20cfa6ea8b062f8b5994551887".into(),
				vec!["3044022066e02c19a513049d49349cf5311a1b012b7c4fae023795a18ab1d91c23496c22022025e216342c8e07ce8ef51e8daee88f84306a9de66236cab230bb63067ded1ad301".into(),
					"410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1896
	#[test]
	fn witness_basic_p2sh_p2wpkh() {
		assert_eq!(Ok(()),
			run_witness_test("16001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				"a91417743beb429c55c942d2ec703b98c4d57c2df5c687".into(),
				vec!["304402200929d11561cd958460371200f82e9cae64c727a495715a31828e27a7ad57b36d0220361732ced04a6f97351ecca21a56d0b8cd4932c1da1f8f569a2b68e5e48aed7801".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1908
	#[test]
	fn witness_basic_p2wsh_with_wrong_key() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("".into(),
				"0020ac8ebd9e52c17619a381fa4f71aebb696087c6ef17c960fd0587addad99c0610".into(),
				vec!["304402202589f0512cb2408fb08ed9bd24f85eb3059744d9e4f2262d0b7f1338cff6e8b902206c0978f449693e0578c71bc543b11079fd0baae700ee5e9a6bee94db490af9fc01".into(),
					"41048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26cafac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1920
	#[test]
	fn witness_basic_p2wpkh_with_wrong_key() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("".into(),
				"00147cf9c846cd4882efec4bf07e44ebdad495c94f4b".into(),
				vec!["304402206ef7fdb2986325d37c6eb1a8bb24aeb46dede112ed8fc76c7d7500b9b83c0d3d02201edc2322c794fe2d6b0bd73ed319e714aa9b86d8891961530d5c9b7156b60d4e01".into(),
					"048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26caf".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1920
	#[test]
	fn witness_basic_p2sh_p2wsh_with_wrong_key() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("220020ac8ebd9e52c17619a381fa4f71aebb696087c6ef17c960fd0587addad99c0610".into(),
				"a91461039a003883787c0d6ebc66d97fdabe8e31449d87".into(),
				vec!["30440220069ea3581afaf8187f63feee1fd2bd1f9c0dc71ea7d6e8a8b07ee2ebcf824bf402201a4fdef4c532eae59223be1eda6a397fc835142d4ddc6c74f4aa85b766a5c16f01".into(),
					"41048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26cafac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1944
	#[test]
	fn witness_basic_p2sh_p2wpkh_with_wrong_key() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("1600147cf9c846cd4882efec4bf07e44ebdad495c94f4b".into(),
				"a9144e0c2aed91315303fc6a1dc4c7bc21c88f75402e87".into(),
				vec!["304402204209e49457c2358f80d0256bc24535b8754c14d08840fc4be762d6f5a0aed80b02202eaf7d8fc8d62f60c67adcd99295528d0e491ae93c195cec5a67e7a09532a88001".into(),
					"048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26caf".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1956
	#[test]
	fn witness_basic_p2wsh_with_wrong_key_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"0020ac8ebd9e52c17619a381fa4f71aebb696087c6ef17c960fd0587addad99c0610".into(),
				vec!["304402202589f0512cb2408fb08ed9bd24f85eb3059744d9e4f2262d0b7f1338cff6e8b902206c0978f449693e0578c71bc543b11079fd0baae700ee5e9a6bee94db490af9fc01".into(),
					"41048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26cafac".into()],
				VerificationFlags::default().verify_p2sh(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1968
	#[test]
	fn witness_basic_p2wpkh_with_wrong_key_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"00147cf9c846cd4882efec4bf07e44ebdad495c94f4b".into(),
				vec!["304402206ef7fdb2986325d37c6eb1a8bb24aeb46dede112ed8fc76c7d7500b9b83c0d3d02201edc2322c794fe2d6b0bd73ed319e714aa9b86d8891961530d5c9b7156b60d4e01".into(),
					"4104828048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26caf2263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26cafac".into()],
				VerificationFlags::default().verify_p2sh(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1980
	#[test]
	fn witness_basic_p2sh_p2wsh_with_wrong_key_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("220020ac8ebd9e52c17619a381fa4f71aebb696087c6ef17c960fd0587addad99c0610".into(),
				"a91461039a003883787c0d6ebc66d97fdabe8e31449d87".into(),
				vec!["30440220069ea3581afaf8187f63feee1fd2bd1f9c0dc71ea7d6e8a8b07ee2ebcf824bf402201a4fdef4c532eae59223be1eda6a397fc835142d4ddc6c74f4aa85b766a5c16f01".into(),
					"41048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26cafac".into()],
				VerificationFlags::default().verify_p2sh(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L1992
	#[test]
	fn witness_basic_p2sh_p2wpkh_with_wrong_key_check_skipped() {
		assert_eq!(Ok(()),
			run_witness_test("1600147cf9c846cd4882efec4bf07e44ebdad495c94f4b".into(),
				"a9144e0c2aed91315303fc6a1dc4c7bc21c88f75402e87".into(),
				vec!["304402204209e49457c2358f80d0256bc24535b8754c14d08840fc4be762d6f5a0aed80b02202eaf7d8fc8d62f60c67adcd99295528d0e491ae93c195cec5a67e7a09532a88001".into(),
					"048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26caf".into()],
				VerificationFlags::default().verify_p2sh(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2004
	#[test]
	fn witness_basic_p2wsh_with_wrong_value() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("".into(),
				"0020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				vec!["3044022066faa86e74e8b30e82691b985b373de4f9e26dc144ec399c4f066aa59308e7c202204712b86f28c32503faa051dbeabff2c238ece861abc36c5e0b40b1139ca222f001".into(),
					"410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2016
	#[test]
	fn witness_basic_p2wpkh_with_wrong_value() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("".into(),
				"001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				vec!["304402203b3389b87448d7dfdb5e82fb854fcf92d7925f9938ea5444e36abef02c3d6a9602202410bc3265049abb07fd2e252c65ab7034d95c9d5acccabe9fadbdc63a52712601".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2028
	#[test]
	fn witness_basic_p2sh_p2wsh_with_wrong_value() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("220020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				"a914f386c2ba255cc56d20cfa6ea8b062f8b5994551887".into(),
				vec!["3044022000a30c4cfc10e4387be528613575434826ad3c15587475e0df8ce3b1746aa210022008149265e4f8e9dafe1f3ea50d90cb425e9e40ea7ebdd383069a7cfa2b77004701".into(),
					"410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2040
	#[test]
	fn witness_basic_p2sh_p2wpkh_with_wrong_value() {
		assert_eq!(Err(Error::EvalFalse),
			run_witness_test("16001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				"a91417743beb429c55c942d2ec703b98c4d57c2df5c687".into(),
				vec!["304402204fc3a2cd61a47913f2a5f9107d0ad4a504c7b31ee2d6b3b2f38c2b10ee031e940220055d58b7c3c281aaa381d8f486ac0f3e361939acfd568046cb6a311cdfa974cf01".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2052
	#[test]
	fn witness_p2wpkh_with_future_version() {
		assert_eq!(Err(Error::DiscourageUpgradableWitnessProgram),
			run_witness_test("".into(),
				"511491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				vec!["304402205ae57ae0534c05ca9981c8a6cdf353b505eaacb7375f96681a2d1a4ba6f02f84022056248e68643b7d8ce7c7d128c9f1f348bcab8be15d094ad5cadd24251a28df8001".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true).verify_discourage_upgradable_witness_program(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2064
	#[test]
	fn witness_p2wpkh_with_wrong_witness_program_length() {
		assert_eq!(Err(Error::WitnessProgramWrongLength),
			run_witness_test("".into(),
				"001fb34b78da162751647974d5cb7410aa428ad339dbf7d1e16e833f68a0cbf1c3".into(),
				vec!["3044022064100ca0e2a33332136775a86cd83d0230e58b9aebb889c5ac952abff79a46ef02205f1bf900e022039ad3091bdaf27ac2aef3eae9ed9f190d821d3e508405b9513101".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2076
	#[test]
	fn witness_p2wsh_with_empty_witness() {
		assert_eq!(Err(Error::WitnessProgramWitnessEmpty),
			run_witness_test("".into(),
				"0020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				vec![],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2083
	#[test]
	fn witness_p2wsh_with_witness_program_mismatch() {
		assert_eq!(Err(Error::WitnessProgramMismatch),
			run_witness_test("".into(),
				"0020b95237b48faaa69eb078e1170be3b5cbb3fddf16d0a991e14ad274f7b33a4f64".into(),
				vec!["3044022039105b995a5f448639a997a5c90fda06f50b49df30c3bdb6663217bf79323db002206fecd54269dec569fcc517178880eb58bb40f381a282bb75766ff3637d5f4b4301".into(),
					"400479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2095
	#[test]
	fn witness_p2wpkh_with_witness_program_mismatch() {
		assert_eq!(Err(Error::WitnessProgramMismatch),
			run_witness_test("".into(),
				"001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				vec!["304402201a96950593cb0af32d080b0f193517f4559241a8ebd1e95e414533ad64a3f423022047f4f6d3095c23235bdff3aeff480d0529c027a3f093cb265b7cbf148553b85101".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into(),
					"".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2108
	#[test]
	fn witness_p2wpkh_with_non_empty_script_sig() {
		assert_eq!(Err(Error::WitnessMalleated),
			run_witness_test("5b".into(),
				"001491b24bf9f5288532960ac687abb035127b1d28a5".into(),
				vec!["304402201a96950593cb0af32d080b0f193517f4559241a8ebd1e95e414533ad64a3f423022047f4f6d3095c23235bdff3aeff480d0529c027a3f093cb265b7cbf148553b85101".into(),
					"0479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2120
	#[test]
	fn witness_p2sh_p2wpkh_with_superfluous_push_in_script_sig() {
		assert_eq!(Err(Error::WitnessMalleatedP2SH),
			run_witness_test("5b1600147cf9c846cd4882efec4bf07e44ebdad495c94f4b".into(),
				"a9144e0c2aed91315303fc6a1dc4c7bc21c88f75402e87".into(),
				vec!["304402204209e49457c2358f80d0256bc24535b8754c14d08840fc4be762d6f5a0aed80b02202eaf7d8fc8d62f60c67adcd99295528d0e491ae93c195cec5a67e7a09532a88001".into(),
					"048282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f5150811f8a8098557dfe45e8256e830b60ace62d613ac2f7b17bed31b6eaff6e26caf".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2132
	#[test]
	fn witness_p2pk_with_witness() {
		assert_eq!(Err(Error::WitnessUnexpected),
			run_witness_test("47304402200a5c6163f07b8d3b013c4d1d6dba25e780b39658d79ba37af7057a3b7f15ffa102201fd9b4eaa9943f734928b99a83592c2e7bf342ea2680f6a2bb705167966b742001".into(),
				"410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b8ac".into(),
				vec!["".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				0,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2299
	#[test]
	fn witness_p2wsh_checkmultisig() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"002008a6665ebfd43b02323423e764e185d98d1587f903b81507dbb69bfc41005efa".into(),
				vec!["".into(),
					"304402202d092ededd1f060609dbf8cb76950634ff42b3e62cf4adb69ab92397b07d742302204ff886f8d0817491a96d1daccdcc820f6feb122ee6230143303100db37dfa79f01".into(),
					"5121038282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f51508410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b852ae".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2312
	#[test]
	fn witness_p2sh_p2wsh_checkmultisig() {
		assert_eq!(Ok(()),
			run_witness_test("22002008a6665ebfd43b02323423e764e185d98d1587f903b81507dbb69bfc41005efa".into(),
				"a9146f5ecd4b83b77f3c438f5214eff96454934fc5d187".into(),
				vec!["".into(),
					"304402202dd7e91243f2235481ffb626c3b7baf2c859ae3a5a77fb750ef97b99a8125dc002204960de3d3c3ab9496e218ec57e5240e0e10a6f9546316fe240c216d45116d29301".into(),
					"5121038282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f51508410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b852ae".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2351
	#[test]
	fn witness_p2wsh_checkmultisig_using_key2() {
		assert_eq!(Ok(()),
			run_witness_test("".into(),
				"002008a6665ebfd43b02323423e764e185d98d1587f903b81507dbb69bfc41005efa".into(),
				vec!["".into(),
					"304402201e9e6f7deef5b2f21d8223c5189b7d5e82d237c10e97165dd08f547c4e5ce6ed02206796372eb1cc6acb52e13ee2d7f45807780bf96b132cb6697f69434be74b1af901".into(),
					"5121038282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f51508410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b852ae".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/script_tests.json#L2364
	#[test]
	fn witness_p2sh_p2wsh_checkmultisig_using_key2() {
		assert_eq!(Ok(()),
			run_witness_test("22002008a6665ebfd43b02323423e764e185d98d1587f903b81507dbb69bfc41005efa".into(),
				"a9146f5ecd4b83b77f3c438f5214eff96454934fc5d187".into(),
				vec!["".into(),
					"3044022045e667f3f0f3147b95597a24babe9afecea1f649fd23637dfa7ed7e9f3ac18440220295748e81005231135289fe3a88338dabba55afa1bdb4478691337009d82b68d01".into(),
					"5121038282263212c609d9ea2a6e3e172de238d8c39cabd5ac1ca10646e23fd5f51508410479be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798483ada7726a3c4655da4fbfc0e1108a8fd17b448a68554199c47d08ffb10d4b852ae".into()],
				VerificationFlags::default().verify_p2sh(true).verify_witness(true),
				1,
			));
	}

	fn run_witness_test_tx_test(script_pubkey: Script, tx: &Transaction, flags: &VerificationFlags, amount: u64, index: usize) -> Result<(), Error> {
		let checker = TransactionSignatureChecker {
			input_index: index,
			input_amount: amount,
			signer: tx.clone().into(),
		};

		verify_script(&tx.inputs[index].script_sig.clone().into(),
			&script_pubkey,
			&tx.inputs[index].script_witness,
			flags,
			&checker,
			SignatureVersion::Base)
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L254
	#[test]
	fn witness_unknown_program_version() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b00000000000001510002483045022100a3cec69b52cba2d2de623ffffffffff1606184ea55476c0f8189fda231bc9cbb022003181ad597f7c380a7d1c740286b1d022b8b04ded028b833282e055e03b8efef812103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true).verify_discourage_upgradable_witness_program(true);
		assert_eq!(Err(Error::DiscourageUpgradableWitnessProgram), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("60144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L260
	#[test]
	fn witness_unknown_program0_lengh() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff04b60300000000000001519e070000000000000151860b0000000000000100960000000000000001510002473044022022fceb54f62f8feea77faac7083c3b56c4676a78f93745adc8a35800bc36adfa022026927df9abcf0a8777829bcfcce3ff0a385fa54c3f9df577405e3ef24ee56479022103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::WitnessProgramWrongLength), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00154c9c3dfac4207d5d8cb89df5722cb3d712385e3fff".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L260
	#[test]
	fn witness_single_anyone_same_index_value_changed() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e80300000000000001516c070000000000000151b80b0000000000000151000248304502210092f4777a0f17bf5aeb8ae768dec5f2c14feabf9d1fe2c89c78dfed0f13fdb86902206da90a86042e252bcd1e80a168c719e4a1ddcc3cebea24b9812c5453c79107e9832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L272
	#[test]
	fn witness_none_anyone_same_index_value_changed() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff000100000000000000000000000000000000000000000000000000000000000001000000000100000000010000000000000000000000000000000000000000000000000000000000000200000000ffffffff00000248304502210091b32274295c2a3fa02f5bce92fb2789e3fc6ea947fbe1a76e52ea3f4ef2381a022079ad72aefa3837a2e0c033a8652a59731da05fa4a813f4fc48e87c075037256b822103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L278
	#[test]
	fn witness_all_anyone_third_value_changed() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151540b00000000000001510002483045022100a3cec69b52cba2d2de623eeef89e0ba1606184ea55476c0f8189fda231bc9cbb022003181ad597f7c380a7d1c740286b1d022b8b04ded028b833282e055e03b8efef812103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L284
	#[test]
	fn witness_with_push_of_521_bytes() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff010000000000000000015102fd0902000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002755100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::PushSize), run_witness_test_tx_test("002033198a9bfef674ebddb9ffaa52928017b8472791e54c609cb95f278ac6b1e349".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L288
	#[test]
	fn witness_unknown_version_with_false_on_stack() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff010000000000000000015101010100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("60020000".into(), &tx, &flags, 2000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L292
	#[test]
	fn witness_unknown_version_with_non_empty_stack() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01000000000000000001510102515100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("00202f04a3aa051f1f60d695f6c44c0c3d383973dfd446ace8962664a76bb10e31a8".into(), &tx, &flags, 2000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L296
	#[test]
	fn witness_program0_with_push_of_2_bytes() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff010000000000000000015101040002000100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::WitnessProgramWrongLength), run_witness_test_tx_test("00020001".into(), &tx, &flags, 2000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L300
	#[test]
	fn witness_unknown_version_with_non_empty_script_sig() {
		let tx = "01000000010001000000000000000000000000000000000000000000000000000000000000000000000151ffffffff010000000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::WitnessMalleated), run_witness_test_tx_test("60020001".into(), &tx, &flags, 2000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L304
	#[test]
	fn witness_non_witness_single_anyone_hash_input_position() {
		let tx = "010000000200010000000000000000000000000000000000000000000000000000000000000100000049483045022100acb96cfdbda6dc94b489fd06f2d720983b5f350e31ba906cdbd800773e80b21c02200d74ea5bdf114212b4bbe9ed82c36d2e369e302dff57cb60d01c428f0bd3daab83ffffffff0001000000000000000000000000000000000000000000000000000000000000000000004847304402202a0b4b1294d70540235ae033d78e64b4897ec859c7b6f1b2b1d8a02e1d46006702201445e756d2254b0f1dfda9ab8e1e1bc26df9668077403204f32d16a49a36eb6983ffffffff02e9030000000000000151e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Err(Error::EvalFalse), run_witness_test_tx_test("2103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("2103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac".into(), &tx, &flags, 1001, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_invalid.json#L313
	#[test]
	fn witness_33_bytes_witness_script_pubkey() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true).verify_discourage_upgradable_witness_program(true);
		assert_eq!(Err(Error::DiscourageUpgradableWitnessProgram), run_witness_test_tx_test("6021ff25429251b5a84f452230a3c75fd886b7fc5a7865ce4a7bb7a9d7c5be6da3dbff".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L320
	#[test]
	fn witness_valid_p2wpkh() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e8030000000000001976a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac02483045022100cfb07164b36ba64c1b1e8c7720a56ad64d96f6ef332d3d37f9cb3c96477dc44502200a464cd7a9cf94cd70f66ce4f4f0625ef650052c7afcfe29d7d7e01830ff91ed012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L324
	#[test]
	fn witness_valid_p2wsh() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e8030000000000001976a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac02483045022100aa5d8aa40a90f23ce2c3d11bc845ca4a12acd99cbea37de6b9f6d86edebba8cb022022dedc2aa0a255f74d04c0b76ece2d7c691f9dd11a64a8ac49f62a99c3a05f9d01232103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("0020ff25429251b5a84f452230a3c75fd886b7fc5a7865ce4a7bb7a9d7c5be6da3db".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L328
	#[test]
	fn witness_valid_p2sh_p2wpkh() {
		let tx = "01000000000101000100000000000000000000000000000000000000000000000000000000000000000000171600144c9c3dfac4207d5d8cb89df5722cb3d712385e3fffffffff01e8030000000000001976a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac02483045022100cfb07164b36ba64c1b1e8c7720a56ad64d96f6ef332d3d37f9cb3c96477dc44502200a464cd7a9cf94cd70f66ce4f4f0625ef650052c7afcfe29d7d7e01830ff91ed012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("a914fe9c7dacc9fcfbf7e3b7d5ad06aa2b28c5a7b7e387".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L328
	#[test]
	fn witness_valid_p2sh_p2wsh() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000023220020ff25429251b5a84f452230a3c75fd886b7fc5a7865ce4a7bb7a9d7c5be6da3dbffffffff01e8030000000000001976a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac02483045022100aa5d8aa40a90f23ce2c3d11bc845ca4a12acd99cbea37de6b9f6d86edebba8cb022022dedc2aa0a255f74d04c0b76ece2d7c691f9dd11a64a8ac49f62a99c3a05f9d01232103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("a9142135ab4f0981830311e35600eebc7376dce3a91487".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L328
	#[test]
	fn witness_valid_single_anyoune() {
		let tx = "0100000000010400010000000000000000000000000000000000000000000000000000000000000200000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000300000000ffffffff05540b0000000000000151d0070000000000000151840300000000000001513c0f00000000000001512c010000000000000151000248304502210092f4777a0f17bf5aeb8ae768dec5f2c14feabf9d1fe2c89c78dfed0f13fdb86902206da90a86042e252bcd1e80a168c719e4a1ddcc3cebea24b9812c5453c79107e9832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71000000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 3100, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 1100, 2))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 4100, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L343
	#[test]
	fn witness_valid_single_anyoune_same_signature() {
		let tx = "0100000000010400010000000000000000000000000000000000000000000000000000000000000200000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000300000000ffffffff05540b0000000000000151d0070000000000000151840300000000000001513c0f00000000000001512c010000000000000151000248304502210092f4777a0f17bf5aeb8ae768dec5f2c14feabf9d1fe2c89c78dfed0f13fdb86902206da90a86042e252bcd1e80a168c719e4a1ddcc3cebea24b9812c5453c79107e9832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71000000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 3100, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 1100, 2))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 4100, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L349
	#[test]
	fn witness_valid_single() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff0484030000000000000151d0070000000000000151540b0000000000000151c800000000000000015100024730440220699e6b0cfe015b64ca3283e6551440a34f901ba62dd4c72fe1cb815afb2e6761022021cc5e84db498b1479de14efda49093219441adc6c543e5534979605e273d80b032103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L355
	#[test]
	fn witness_valid_single_same_signature() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b000000000000015100024730440220699e6b0cfe015b64ca3283e6551440a34f901ba62dd4c72fe1cb815afb2e6761022021cc5e84db498b1479de14efda49093219441adc6c543e5534979605e273d80b032103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L361
	#[test]
	fn witness_valid_none_anyone() {
		let tx = "0100000000010400010000000000000000000000000000000000000000000000000000000000000200000000ffffffff00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000300000000ffffffff04b60300000000000001519e070000000000000151860b00000000000001009600000000000000015100000248304502210091b32274295c2a3fa02f5bce92fb2789e3fc6ea947fbe1a76e52ea3f4ef2381a022079ad72aefa3837a2e0c033a8652a59731da05fa4a813f4fc48e87c075037256b822103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 3100, 0)
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 1100, 1))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 2))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 4100, 3)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L368
	#[test]
	fn witness_valid_none_anyone_same_signature() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b0000000000000151000248304502210091b32274295c2a3fa02f5bce92fb2789e3fc6ea947fbe1a76e52ea3f4ef2381a022079ad72aefa3837a2e0c033a8652a59731da05fa4a813f4fc48e87c075037256b822103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L374
	#[test]
	fn witness_none() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff04b60300000000000001519e070000000000000151860b0000000000000100960000000000000001510002473044022022fceb54f62f8feea77faac7083c3b56c4676a78f93745adc8a35800bc36adfa022026927df9abcf0a8777829bcfcce3ff0a385fa54c3f9df577405e3ef24ee56479022103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L380
	#[test]
	fn witness_none_same_signature() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b00000000000001510002473044022022fceb54f62f8feea77faac7083c3b56c4676a78f93745adc8a35800bc36adfa022026927df9abcf0a8777829bcfcce3ff0a385fa54c3f9df577405e3ef24ee56479022103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L386
	#[test]
	fn witness_none_same_signature_sequence_changed() {
		let tx = "01000000000103000100000000000000000000000000000000000000000000000000000000000000000000000200000000010000000000000000000000000000000000000000000000000000000000000100000000ffffffff000100000000000000000000000000000000000000000000000000000000000002000000000200000003e8030000000000000151d0070000000000000151b80b00000000000001510002473044022022fceb54f62f8feea77faac7083c3b56c4676a78f93745adc8a35800bc36adfa022026927df9abcf0a8777829bcfcce3ff0a385fa54c3f9df577405e3ef24ee56479022103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L392
	#[test]
	fn witness_all_anyone() {
		let tx = "0100000000010400010000000000000000000000000000000000000000000000000000000000000200000000ffffffff00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000300000000ffffffff03e8030000000000000151d0070000000000000151b80b0000000000000151000002483045022100a3cec69b52cba2d2de623eeef89e0ba1606184ea55476c0f8189fda231bc9cbb022003181ad597f7c380a7d1c740286b1d022b8b04ded028b833282e055e03b8efef812103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 3100, 0)
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 1100, 1))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 2))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 4100, 3)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L399
	#[test]
	fn witness_all_anyone_same_signature() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b00000000000001510002483045022100a3cec69b52cba2d2de623eeef89e0ba1606184ea55476c0f8189fda231bc9cbb022003181ad597f7c380a7d1c740286b1d022b8b04ded028b833282e055e03b8efef812103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L405
	#[test]
	fn witness_unknown_witness_program_version() {
		let tx = "0100000000010300010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000200000000ffffffff03e8030000000000000151d0070000000000000151b80b00000000000001510002483045022100a3cec69b52cba2d2de623ffffffffff1606184ea55476c0f8189fda231bc9cbb022003181ad597f7c380a7d1c740286b1d022b8b04ded028b833282e055e03b8efef812103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("60144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 2000, 1))
			.and_then(|_| run_witness_test_tx_test("51".into(), &tx, &flags, 3000, 2)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L411
	#[test]
	fn witness_push_520_bytes() {
		let tx = "0100000000010100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff010000000000000000015102fd08020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000002755100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("002033198a9bfef674ebddb9ffaa52928017b8472791e54c609cb95f278ac6b1e349".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L415
	#[test]
	fn witness_mixed_transaction() {
		let tx = "0100000000010c00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff0001000000000000000000000000000000000000000000000000000000000000020000006a473044022026c2e65b33fcd03b2a3b0f25030f0244bd23cc45ae4dec0f48ae62255b1998a00220463aa3982b718d593a6b9e0044513fd67a5009c2fdccc59992cffc2b167889f4012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff0001000000000000000000000000000000000000000000000000000000000000030000006a4730440220008bd8382911218dcb4c9f2e75bf5c5c3635f2f2df49b36994fde85b0be21a1a02205a539ef10fb4c778b522c1be852352ea06c67ab74200977c722b0bc68972575a012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff0001000000000000000000000000000000000000000000000000000000000000040000006b483045022100d9436c32ff065127d71e1a20e319e4fe0a103ba0272743dbd8580be4659ab5d302203fd62571ee1fe790b182d078ecfd092a509eac112bea558d122974ef9cc012c7012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff0001000000000000000000000000000000000000000000000000000000000000050000006a47304402200e2c149b114ec546015c13b2b464bbcb0cdc5872e6775787527af6cbc4830b6c02207e9396c6979fb15a9a2b96ca08a633866eaf20dc0ff3c03e512c1d5a1654f148012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff0001000000000000000000000000000000000000000000000000000000000000060000006b483045022100b20e70d897dc15420bccb5e0d3e208d27bdd676af109abbd3f88dbdb7721e6d6022005836e663173fbdfe069f54cde3c2decd3d0ea84378092a5d9d85ec8642e8a41012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff00010000000000000000000000000000000000000000000000000000000000000700000000ffffffff00010000000000000000000000000000000000000000000000000000000000000800000000ffffffff00010000000000000000000000000000000000000000000000000000000000000900000000ffffffff00010000000000000000000000000000000000000000000000000000000000000a00000000ffffffff00010000000000000000000000000000000000000000000000000000000000000b0000006a47304402206639c6e05e3b9d2675a7f3876286bdf7584fe2bbd15e0ce52dd4e02c0092cdc60220757d60b0a61fc95ada79d23746744c72bac1545a75ff6c2c7cdb6ae04e7e9592012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ffffffff0ce8030000000000000151e9030000000000000151ea030000000000000151eb030000000000000151ec030000000000000151ed030000000000000151ee030000000000000151ef030000000000000151f0030000000000000151f1030000000000000151f2030000000000000151f30300000000000001510248304502210082219a54f61bf126bfc3fa068c6e33831222d1d7138c6faa9d33ca87fd4202d6022063f9902519624254d7c2c8ea7ba2d66ae975e4e229ae38043973ec707d5d4a83012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7102473044022017fb58502475848c1b09f162cb1688d0920ff7f142bed0ef904da2ccc88b168f02201798afa61850c65e77889cbcd648a5703b487895517c88f85cdd18b021ee246a012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7100000000000247304402202830b7926e488da75782c81a54cd281720890d1af064629ebf2e31bf9f5435f30220089afaa8b455bbeb7d9b9c3fe1ed37d07685ade8455c76472cda424d93e4074a012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7102473044022026326fcdae9207b596c2b05921dbac11d81040c4d40378513670f19d9f4af893022034ecd7a282c0163b89aaa62c22ec202cef4736c58cd251649bad0d8139bcbf55012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71024730440220214978daeb2f38cd426ee6e2f44131a33d6b191af1c216247f1dd7d74c16d84a02205fdc05529b0bc0c430b4d5987264d9d075351c4f4484c16e91662e90a72aab24012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710247304402204a6e9f199dc9672cf2ff8094aaa784363be1eb62b679f7ff2df361124f1dca3302205eeb11f70fab5355c9c8ad1a0700ea355d315e334822fa182227e9815308ee8f012103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710000000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1001, 1))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1002, 2))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1003, 3))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1004, 4))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1005, 5))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1006, 6))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1007, 7))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1008, 8))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1009, 9))
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1010, 10))
			.and_then(|_| run_witness_test_tx_test("76a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac".into(), &tx, &flags, 1011, 11)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L430
	#[test]
	fn witness_unknown_version_with_empty_witness() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("60144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L434
	#[test]
	fn witness_single_output_oob() {
		let tx = "0100000000010200010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff01d00700000000000001510003483045022100e078de4e96a0e05dcdc0a414124dd8475782b5f3f0ed3f607919e9a5eeeb22bf02201de309b3a3109adb3de8074b3610d4cf454c49b61247a2779a0bcbf31c889333032103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc711976a9144c9c3dfac4207d5d8cb89df5722cb3d712385e3f88ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("51".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00204d6c2a32c87821d68fc016fca70797abdb80df6cd84651d40a9300c6bad79e62".into(), &tx, &flags, 1000, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L439
	#[test]
	fn witness_1_byte_push_not_witness_script_pubkey() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("600101".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L443
	#[test]
	fn witness_41_byte_push_not_witness_script_pubkey() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("6029ff25429251b5a84f452230a3c75fd886b7fc5a7865ce4a7bb7a9d7c5be6da3dbff0000000000000000".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L447
	#[test]
	fn witness_version_must_use_op1_to_op16() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("0110020001".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L451
	#[test]
	fn witness_program_push_must_be_canonical() {
		let tx = "010000000100010000000000000000000000000000000000000000000000000000000000000000000000ffffffff01e803000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("604c020001".into(), &tx, &flags, 1000, 0));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L455
	#[test]
	fn witness_single_anyone_does_not_hash_input_position() {
		let tx = "0100000000010200010000000000000000000000000000000000000000000000000000000000000000000000ffffffff00010000000000000000000000000000000000000000000000000000000000000100000000ffffffff02e8030000000000000151e90300000000000001510247304402206d59682663faab5e4cb733c562e22cdae59294895929ec38d7c016621ff90da0022063ef0af5f970afe8a45ea836e3509b8847ed39463253106ac17d19c437d3d56b832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710248304502210085001a820bfcbc9f9de0298af714493f8a37b3b354bfd21a7097c3e009f2018c022050a8b4dbc8155d4d04da2f5cdd575dcf8dd0108de8bec759bd897ea01ecb3af7832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1001, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L460
	#[test]
	fn witness_single_anyone_does_not_hash_input_position_permutation() {
		let tx = "0100000000010200010000000000000000000000000000000000000000000000000000000000000100000000ffffffff00010000000000000000000000000000000000000000000000000000000000000000000000ffffffff02e9030000000000000151e80300000000000001510248304502210085001a820bfcbc9f9de0298af714493f8a37b3b354bfd21a7097c3e009f2018c022050a8b4dbc8155d4d04da2f5cdd575dcf8dd0108de8bec759bd897ea01ecb3af7832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc710247304402206d59682663faab5e4cb733c562e22cdae59294895929ec38d7c016621ff90da0022063ef0af5f970afe8a45ea836e3509b8847ed39463253106ac17d19c437d3d56b832103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc7100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1001, 0)
			.and_then(|_| run_witness_test_tx_test("00144c9c3dfac4207d5d8cb89df5722cb3d712385e3f".into(), &tx, &flags, 1000, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L465
	#[test]
	fn witness_non_witness_single_anyone_hash_input_position_ok() {
		let tx = "01000000020001000000000000000000000000000000000000000000000000000000000000000000004847304402202a0b4b1294d70540235ae033d78e64b4897ec859c7b6f1b2b1d8a02e1d46006702201445e756d2254b0f1dfda9ab8e1e1bc26df9668077403204f32d16a49a36eb6983ffffffff00010000000000000000000000000000000000000000000000000000000000000100000049483045022100acb96cfdbda6dc94b489fd06f2d720983b5f350e31ba906cdbd800773e80b21c02200d74ea5bdf114212b4bbe9ed82c36d2e369e302dff57cb60d01c428f0bd3daab83ffffffff02e8030000000000000151e903000000000000015100000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("2103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac".into(), &tx, &flags, 1000, 0)
			.and_then(|_| run_witness_test_tx_test("2103596d3451025c19dbbdeb932d6bf8bfb4ad499b95b6f88db8899efac102e5fc71ac".into(), &tx, &flags, 1001, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L471
	#[test]
	fn witness_bip143_example1() {
		let tx = "01000000000102fe3dc9208094f3ffd12645477b3dc56f60ec4fa8e6f5d67c565d1c6b9216b36e000000004847304402200af4e47c9b9629dbecc21f73af989bdaa911f7e6f6c2e9394588a3aa68f81e9902204f3fcf6ade7e5abb1295b6774c8e0abd94ae62217367096bc02ee5e435b67da201ffffffff0815cf020f013ed6cf91d29f4202e8a58726b1ac6c79da47c23d1bee0a6925f80000000000ffffffff0100f2052a010000001976a914a30741f8145e5acadf23f751864167f32e0963f788ac000347304402200de66acf4527789bfda55fc5459e214fa6083f936b430a762c629656216805ac0220396f550692cd347171cbc1ef1f51e15282e837bb2b30860dc77c8f78bc8501e503473044022027dc95ad6b740fe5129e7e62a75dd00f291a2aeb1200b84b09d9e3789406b6c002201a9ecd315dd6a0e632ab20bbb98948bc0c6fb204f2c286963bb48517a7058e27034721026dccc749adc2a9d0d89497ac511f760f45c47dc5ed9cf352a58ac706453880aeadab210255a9626aebf5e29c0e6538428ba0d1dcf6ca98ffdf086aa8ced5e0d0215ea465ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("21036d5c20fa14fb2f635474c1dc4ef5909d4568e5569b79fc94d3448486e14685f8ac".into(), &tx, &flags, 156250000, 0)
			.and_then(|_| run_witness_test_tx_test("00205d1b56b63d714eebe542309525f484b7e9d6f686b3781b6f61ef925d66d6f6a0".into(), &tx, &flags, 4900000000, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L476
	#[test]
	fn witness_bip143_example2() {
		let tx = "01000000000102e9b542c5176808107ff1df906f46bb1f2583b16112b95ee5380665ba7fcfc0010000000000ffffffff80e68831516392fcd100d186b3c2c7b95c80b53c77e77c35ba03a66b429a2a1b0000000000ffffffff0280969800000000001976a914de4b231626ef508c9a74a8517e6783c0546d6b2888ac80969800000000001976a9146648a8cd4531e1ec47f35916de8e259237294d1e88ac02483045022100f6a10b8604e6dc910194b79ccfc93e1bc0ec7c03453caaa8987f7d6c3413566002206216229ede9b4d6ec2d325be245c5b508ff0339bf1794078e20bfe0babc7ffe683270063ab68210392972e2eb617b2388771abe27235fd5ac44af8e61693261550447a4c3e39da98ac024730440220032521802a76ad7bf74d0e2c218b72cf0cbc867066e2e53db905ba37f130397e02207709e2188ed7f08f4c952d9d13986da504502b8c3be59617e043552f506c46ff83275163ab68210392972e2eb617b2388771abe27235fd5ac44af8e61693261550447a4c3e39da98ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("0020ba468eea561b26301e4cf69fa34bde4ad60c81e70f059f045ca9a79931004a4d".into(), &tx, &flags, 16777215, 0)
			.and_then(|_| run_witness_test_tx_test("0020d9bbfbe56af7c4b7f960a70d7ea107156913d9e5a26b0a71429df5e097ca6537".into(), &tx, &flags, 16777215, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L481
	#[test]
	fn witness_bip143_example3() {
		let tx = "0100000000010280e68831516392fcd100d186b3c2c7b95c80b53c77e77c35ba03a66b429a2a1b0000000000ffffffffe9b542c5176808107ff1df906f46bb1f2583b16112b95ee5380665ba7fcfc0010000000000ffffffff0280969800000000001976a9146648a8cd4531e1ec47f35916de8e259237294d1e88ac80969800000000001976a914de4b231626ef508c9a74a8517e6783c0546d6b2888ac024730440220032521802a76ad7bf74d0e2c218b72cf0cbc867066e2e53db905ba37f130397e02207709e2188ed7f08f4c952d9d13986da504502b8c3be59617e043552f506c46ff83275163ab68210392972e2eb617b2388771abe27235fd5ac44af8e61693261550447a4c3e39da98ac02483045022100f6a10b8604e6dc910194b79ccfc93e1bc0ec7c03453caaa8987f7d6c3413566002206216229ede9b4d6ec2d325be245c5b508ff0339bf1794078e20bfe0babc7ffe683270063ab68210392972e2eb617b2388771abe27235fd5ac44af8e61693261550447a4c3e39da98ac00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("0020d9bbfbe56af7c4b7f960a70d7ea107156913d9e5a26b0a71429df5e097ca6537".into(), &tx, &flags, 16777215, 0)
			.and_then(|_| run_witness_test_tx_test("0020ba468eea561b26301e4cf69fa34bde4ad60c81e70f059f045ca9a79931004a4d".into(), &tx, &flags, 16777215, 1)));
	}

	// https://github.com/bitcoin/bitcoin/blob/7ee6c434ce8df9441abcf1718555cc7728a4c575/src/test/data/tx_valid.json#L486
	#[test]
	fn witness_bip143_example4() {
		let tx = "0100000000010136641869ca081e70f394c6948e8af409e18b619df2ed74aa106c1ca29787b96e0100000023220020a16b5755f7f6f96dbd65f5f0d6ab9418b89af4b1f14a1bb8a09062c35f0dcb54ffffffff0200e9a435000000001976a914389ffce9cd9ae88dcc0631e88a821ffdbe9bfe2688acc0832f05000000001976a9147480a33f950689af511e6e84c138dbbd3c3ee41588ac080047304402206ac44d672dac41f9b00e28f4df20c52eeb087207e8d758d76d92c6fab3b73e2b0220367750dbbe19290069cba53d096f44530e4f98acaa594810388cf7409a1870ce01473044022068c7946a43232757cbdf9176f009a928e1cd9a1a8c212f15c1e11ac9f2925d9002205b75f937ff2f9f3c1246e547e54f62e027f64eefa2695578cc6432cdabce271502473044022059ebf56d98010a932cf8ecfec54c48e6139ed6adb0728c09cbe1e4fa0915302e022007cd986c8fa870ff5d2b3a89139c9fe7e499259875357e20fcbb15571c76795403483045022100fbefd94bd0a488d50b79102b5dad4ab6ced30c4069f1eaa69a4b5a763414067e02203156c6a5c9cf88f91265f5a942e96213afae16d83321c8b31bb342142a14d16381483045022100a5263ea0553ba89221984bd7f0b13613db16e7a70c549a86de0cc0444141a407022005c360ef0ae5a5d4f9f2f87a56c1546cc8268cab08c73501d6b3be2e1e1a8a08824730440220525406a1482936d5a21888260dc165497a90a15669636d8edca6b9fe490d309c022032af0c646a34a44d1f4576bf6a4a74b67940f8faa84c7df9abe12a01a11e2b4783cf56210307b8ae49ac90a048e9b53357a2354b3334e9c8bee813ecb98e99a7e07e8c3ba32103b28f0c28bfab54554ae8c658ac5c3e0ce6e79ad336331f78c428dd43eea8449b21034b8113d703413d57761b8b9781957b8c0ac1dfe69f492580ca4195f50376ba4a21033400f6afecb833092a9a21cfdf1ed1376e58c5d1f47de74683123987e967a8f42103a6d48b1131e94ba04d9737d61acdaa1322008af9602b3b14862c07a1789aac162102d8b661b0b3302ee2f162b09e07a55ad5dfbe673a9f01d9f0c19617681024306b56ae00000000".into();
		let flags = VerificationFlags::default().verify_witness(true).verify_p2sh(true);
		assert_eq!(Ok(()), run_witness_test_tx_test("a9149993a429037b5d912407a71c252019287b8d27a587".into(), &tx, &flags, 987654321, 0));
	}

	#[test]
	fn op_cat_disabled_by_default() {
		let script = Builder::default()
			.push_data(&[1; 1])
			.push_data(&[1; 1])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_CAT));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_cat_max_and_non_empty_succeeds() {
		// maxlen_x empty OP_CAT → ok
		let script = Builder::default()
			.push_data(&[1; MAX_SCRIPT_ELEMENT_SIZE])
			.push_data(&[1; 0])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![vec![1; MAX_SCRIPT_ELEMENT_SIZE].into()].into());
	}

	#[test]
	fn op_cat_max_and_non_empty_fails() {
		// maxlen_x y OP_CAT → failure
		let script = Builder::default()
			.push_data(&[1; MAX_SCRIPT_ELEMENT_SIZE])
			.push_data(&[1; 1])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Err(Error::PushSize);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![].into());
	}

	#[test]
	fn op_cat_large_and_large_fails() {
		// large_x large_y OP_CAT → failure
		let script = Builder::default()
			.push_data(&[1; MAX_SCRIPT_ELEMENT_SIZE / 2 + 1])
			.push_data(&[1; MAX_SCRIPT_ELEMENT_SIZE / 2 + 1])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Err(Error::PushSize);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![].into());
	}

	#[test]
	fn op_cat_empty_and_empty_succeeds() {
		// OP_0 OP_0 OP_CAT → OP_0
		let script = Builder::default()
			.push_opcode(Opcode::OP_0)
			.push_opcode(Opcode::OP_0)
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Ok(false);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![Bytes::default()].into());
	}

	#[test]
	fn op_cat_non_empty_and_empty_succeeds() {
		// x OP_0 OP_CAT → x
		let script = Builder::default()
			.push_data(&[1; 1])
			.push_opcode(Opcode::OP_0)
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![vec![1; 1].into()].into());
	}

	#[test]
	fn op_cat_empty_and_non_empty_succeeds() {
		// OP_0 x OP_CAT → x
		let script = Builder::default()
			.push_opcode(Opcode::OP_0)
			.push_data(&[1; 1])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![vec![1; 1].into()].into());
	}

	#[test]
	fn op_cat_non_empty_and_non_empty_succeeds() {
		// x y OP_CAT → concat(x,y)
		let script = Builder::default()
			.push_data(&[0x11])
			.push_data(&[0x22, 0x33])
			.push_opcode(Opcode::OP_CAT)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_concat(true), result,
			vec![vec![0x11, 0x22, 0x33].into()].into());
	}

	#[test]
	fn op_split_disabled_by_default() {
		let script = Builder::default()
			.push_data(&[0x11, 0x22])
			.push_num(1.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_SUBSTR));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_split_empty_at_zero_succeeds() {
		// OP_0 0 OP_SPLIT -> OP_0 OP_0
		let script = Builder::default()
			.push_opcode(Opcode::OP_0)
			.push_num(0.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Ok(false);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![vec![0; 0].into(), vec![0; 0].into()].into());
	}

	#[test]
	fn op_split_non_empty_at_zero_succeeds() {
		// x 0 OP_SPLIT -> OP_0 x
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_num(0.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![vec![0; 0].into(), vec![0x00, 0x11, 0x22].into()].into());
	}

	#[test]
	fn op_split_non_empty_at_len_succeeds() {
		// x len(x) OP_SPLIT -> x OP_0
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_num(3.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Ok(false);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![vec![0x00, 0x11, 0x22].into(), vec![0; 0].into()].into());
	}

	#[test]
	fn op_split_non_empty_at_post_len_fails() {
		// x (len(x) + 1) OP_SPLIT -> FAIL
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_num(4.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Err(Error::InvalidSplitRange);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![].into());
	}

	#[test]
	fn op_split_non_empty_at_mid_succeeds() {
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![vec![0x00, 0x11].into(), vec![0x22].into()].into());
	}

	#[test]
	fn op_split_fails_if_position_is_nan() {
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_opcode(Opcode::OP_1NEGATE) // NaN
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![].into());
	}

	#[test]
	fn op_split_fails_if_position_is_negative() {
		let script = Builder::default()
			.push_data(&[0x00, 0x11, 0x22])
			.push_num((-10).into())
			.push_opcode(Opcode::OP_SUBSTR)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_split(true), result,
			vec![].into());
	}

	#[test]
	fn op_and_disabled_by_default() {
		let script = Builder::default()
			.push_data(&[0x11])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_AND)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_AND));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_and_fails_with_different_len_args() {
		let script = Builder::default()
			.push_data(&[0x11, 0x22])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_AND)
			.into_script();
		let result = Err(Error::InvalidOperandSize);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_and(true), result,
			vec![].into());
	}

	#[test]
	fn op_and_succeeds() {
		let script = Builder::default()
			.push_data(&[0x34, 0x56])
			.push_data(&[0x56, 0x78])
			.push_opcode(Opcode::OP_AND)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_and(true), result,
			vec![vec![0x14, 0x50].into()].into());
	}

	#[test]
	fn op_or_disabled_by_default() {
		let script = Builder::default()
			.push_data(&[0x11])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_OR)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_OR));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_or_fails_with_different_len_args() {
		let script = Builder::default()
			.push_data(&[0x11, 0x22])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_OR)
			.into_script();
		let result = Err(Error::InvalidOperandSize);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_or(true), result,
			vec![].into());
	}

	#[test]
	fn op_or_succeeds() {
		let script = Builder::default()
			.push_data(&[0x34, 0x56])
			.push_data(&[0x56, 0x78])
			.push_opcode(Opcode::OP_OR)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_or(true), result,
			vec![vec![0x76, 0x7e].into()].into());
	}

	#[test]
	fn op_xor_disabled_by_default() {
		let script = Builder::default()
			.push_data(&[0x11])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_XOR)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_XOR));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_xor_fails_with_different_len_args() {
		let script = Builder::default()
			.push_data(&[0x11, 0x22])
			.push_data(&[0x22])
			.push_opcode(Opcode::OP_XOR)
			.into_script();
		let result = Err(Error::InvalidOperandSize);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_xor(true), result,
			vec![].into());
	}

	#[test]
	fn op_xor_succeeds() {
		let script = Builder::default()
			.push_data(&[0x34, 0x56])
			.push_data(&[0x56, 0x78])
			.push_opcode(Opcode::OP_XOR)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_xor(true), result,
			vec![vec![0x62, 0x2e].into()].into());
	}

	#[test]
	fn op_div_disabled_by_default() {
		let script = Builder::default()
			.push_num(13.into())
			.push_num(5.into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_DIV));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_div_num_by_nan_fails() {
		// a b OP_DIV -> failure where !isnum(a) or !isnum(b). Both operands must be valid numbers
		let script = Builder::default()
			.push_opcode(Opcode::OP_1SUB)
			.push_num(5.into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![].into());
	}

	#[test]
	fn op_div_nan_by_num_fails() {
		// a b OP_DIV -> failure where !isnum(a) or !isnum(b). Both operands must be valid numbers
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1SUB)
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![].into());
	}

	#[test]
	fn op_div_num_by_zero_fails() {
		// a 0 OP_DIV -> failure. Division by positive zero (all sizes), negative zero (all sizes), OP_0
		let script = Builder::default()
			.push_num(0.into())
			.push_num(5.into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Err(Error::DivisionByZero);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![].into());
	}

	#[test]
	fn op_div_negative_by_negative_succeeds() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_num((-13).into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![Num::from(2).to_bytes()].into());
	}

	#[test]
	fn op_div_negative_by_positive_succeeds() {
		let script = Builder::default()
			.push_num(5.into())
			.push_num((-13).into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![Num::from(-2).to_bytes()].into());
	}

	#[test]
	fn op_div_positive_by_negative_succeeds() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_num(13.into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![Num::from(-2).to_bytes()].into());
	}

	#[test]
	fn op_div_positive_by_positive_succeeds() {
		let script = Builder::default()
			.push_num(5.into())
			.push_num(13.into())
			.push_opcode(Opcode::OP_DIV)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_div(true), result,
			vec![Num::from(2).to_bytes()].into());
	}

	#[test]
	fn op_mod_disabled_by_default() {
		let script = Builder::default()
			.push_num(13.into())
			.push_num(5.into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_MOD));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn op_mod_num_by_nan_fails() {
		// a b OP_MOD -> failure where !isnum(a) or !isnum(b). Both operands must be valid numbers
		let script = Builder::default()
			.push_opcode(Opcode::OP_1SUB)
			.push_num(5.into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![].into());
	}

	#[test]
	fn op_mod_nan_by_num_fails() {
		// a b OP_MOD -> failure where !isnum(a) or !isnum(b). Both operands must be valid numbers
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1SUB)
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![].into());
	}

	#[test]
	fn op_mod_num_by_zero_fails() {
		// a 0 OP_MOD -> failure. Division by positive zero (all sizes), negative zero (all sizes), OP_0
		let script = Builder::default()
			.push_num(0.into())
			.push_num(5.into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Err(Error::DivisionByZero);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![].into());
	}

	#[test]
	fn op_mod_negative_by_negative_succeeds() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_num((-13).into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![Num::from(-3).to_bytes()].into());
	}

	#[test]
	fn op_mod_negative_by_positive_succeeds() {
		let script = Builder::default()
			.push_num(5.into())
			.push_num((-13).into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![Num::from(-3).to_bytes()].into());
	}

	#[test]
	fn op_mod_positive_by_negative_succeeds() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_num(13.into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![Num::from(3).to_bytes()].into());
	}

	#[test]
	fn op_mod_positive_by_positive_succeeds() {
		let script = Builder::default()
			.push_num(5.into())
			.push_num(13.into())
			.push_opcode(Opcode::OP_MOD)
			.into_script();
		let result = Ok(true);
		basic_test_with_flags(&script, &VerificationFlags::default().verify_mod(true), result,
			vec![Num::from(3).to_bytes()].into());
	}

	#[test]
	fn op_bin2num_disabled_by_default() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_RIGHT)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_RIGHT));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn test_bin2num_all() {
		fn test_bin2num(input: &[u8], result: Result<bool, Error>, output: Vec<u8>) {
			let script = Builder::default()
				.push_bytes(input)
				.push_opcode(Opcode::OP_RIGHT)
				.into_script();
			let stack = if result.is_ok() {
				vec![output.into()].into()
			} else {
				vec![]
			}.into();
			let flags = VerificationFlags::default()
				.verify_bin2num(true);
			basic_test_with_flags(&script, &flags, result, stack);
		}

		test_bin2num(&[0x02, 0x00, 0x00, 0x00, 0x00], Ok(true), vec![0x02]);
		test_bin2num(&[0x05, 0x00, 0x80], Ok(true), vec![0x85]);
		test_bin2num(&[0x02, 0x02, 0x02, 0x02, 0x02], Err(Error::NumberOverflow), vec![]);
		test_bin2num(&[0x00], Ok(false), vec![]);
		test_bin2num(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00], Ok(true), vec![0x01]);
		test_bin2num(&[0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80], Ok(true), vec![0x81]);
		test_bin2num(&[0x80], Ok(false), vec![]);
		test_bin2num(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80], Ok(false), vec![]);
	}

	#[test]
	fn op_num2bin_disabled_by_default() {
		let script = Builder::default()
			.push_num(1.into())
			.push_num(1.into())
			.push_opcode(Opcode::OP_LEFT)
			.into_script();
		let result = Err(Error::DisabledOpcode(Opcode::OP_LEFT));
		basic_test_with_flags(&script, &VerificationFlags::default(), result,
			vec![].into());
	}

	#[test]
	fn test_num2bin_all() {
		fn test_num2bin(num: &[u8], size: &[u8], result: Result<bool, Error>, output: Vec<u8>) {
			let script = Builder::default()
				.push_data(num)
				.push_data(size)
				.push_opcode(Opcode::OP_LEFT)
				.into_script();
			let stack = if result.is_ok() {
				vec![output.into()].into()
			} else {
				vec![]
			}.into();

			let flags = VerificationFlags::default()
				.verify_num2bin(true);
			basic_test_with_flags(&script, &flags, result, stack);
		}

		fn test_num2bin_num(num: Num, size: Num, result: Result<bool, Error>, output: Vec<u8>) {
			test_num2bin(&*num.to_bytes(), &*size.to_bytes(), result, output)
		}

		test_num2bin_num(256.into(), 1.into(), Err(Error::ImpossibleEncoding), vec![0x00]);
		test_num2bin_num(1.into(), (MAX_SCRIPT_ELEMENT_SIZE + 1).into(), Err(Error::PushSize), vec![0x00]);

		test_num2bin_num(0.into(), 0.into(), Ok(false), vec![]);
		test_num2bin_num(0.into(), 1.into(), Ok(false), vec![0x00]);
		test_num2bin_num(0.into(), 7.into(), Ok(false), vec![0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
		test_num2bin_num(1.into(), 1.into(), Ok(true), vec![0x01]);
		test_num2bin_num((-42).into(), 1.into(), Ok(true), Num::from(-42).to_bytes().to_vec());
		test_num2bin_num((-42).into(), 2.into(), Ok(true), vec![0x2a, 0x80]);
		test_num2bin_num((-42).into(), 10.into(), Ok(true), vec![0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80]);
		test_num2bin_num((-42).into(), 520.into(), Ok(true), ::std::iter::once(0x2a)
			.chain(::std::iter::repeat(0x00).take(518))
			.chain(::std::iter::once(0x80)).collect());
		test_num2bin_num((-42).into(), 521.into(), Err(Error::PushSize), vec![]);
		test_num2bin_num((-42).into(), (-3).into(), Err(Error::PushSize), vec![]);

		test_num2bin(&vec![0xab, 0xcd, 0xef, 0x42, 0x80], &vec![0x04], Ok(true), vec![0xab, 0xcd, 0xef, 0xc2]);
		test_num2bin(&vec![0x80], &vec![0x00], Ok(false), vec![]);
		test_num2bin(&vec![0x80], &vec![0x03], Ok(false), vec![0x00, 0x00, 0x00]);
	}

	#[test]
	fn test_num_bin_conversions_are_reverse_ops() {
		let script = Builder::default()
			// convert num2bin
			.push_num(123456789.into())
			.push_num(8.into())
			.push_opcode(Opcode::OP_LEFT)
			// and then back bin2num
			.push_opcode(Opcode::OP_RIGHT)
			// check that numbers are the same
			.push_num(123456789.into())
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();

		let flags = VerificationFlags::default()
			.verify_num2bin(true)
			.verify_bin2num(true);
		basic_test_with_flags(&script, &flags, Ok(true), vec![vec![0x01].into()].into());
	}

	#[test]
	fn test_split_cat_are_reverse_ops() {
		let script = Builder::default()
			// split array
			.push_data(&vec![0x01, 0x02, 0x03, 0x04, 0x05])
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUBSTR)
			// and then concat again
			.push_opcode(Opcode::OP_CAT)
			// check that numbers are the same
			.push_data(&vec![0x01, 0x02, 0x03, 0x04, 0x05])
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();

		let flags = VerificationFlags::default()
			.verify_concat(true)
			.verify_split(true);
		basic_test_with_flags(&script, &flags, Ok(true), vec![vec![0x01].into()].into());
	}

	#[test]
	fn checkdatasig_spec_tests() {
		// official tests from:
		// https://github.com/bitcoincashorg/bitcoincash.org/blob/0c6f91b0b713aae3bc6c9834b46e80e247ff5fab/spec/op_checkdatasig.md

		let kp = KeyPair::from_private(Private { network: Network::Mainnet, secret: 1.into(), compressed: false, }).unwrap();

		let pubkey = kp.public().clone();
		let message = vec![42u8; 32];
		let correct_signature = kp.private().sign(&Message::from(sha256(&message))).unwrap();
		let correct_signature_for_other_message = kp.private().sign(&[43u8; 32].into()).unwrap();
		let mut correct_signature = correct_signature.to_vec();
		let mut correct_signature_for_other_message = correct_signature_for_other_message.to_vec();
		correct_signature.push(0x81);
		correct_signature_for_other_message.push(0x81);

		let correct_flags = VerificationFlags::default()
			.verify_checkdatasig(true)
			.verify_dersig(true)
			.verify_strictenc(true);
		let incorrect_flags = VerificationFlags::default().verify_checkdatasig(false);

		let correct_signature_script = Builder::default()
			.push_data(&*correct_signature)
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();

		// <sig> <msg> <pubKey> OP_CHECKDATASIG fails if 15 November 2018 protocol upgrade is not yet activated.
		basic_test_with_flags(&correct_signature_script, &incorrect_flags, Err(Error::DisabledOpcode(Opcode::OP_CHECKDATASIG)), vec![].into());

		// <sig> <msg> OP_CHECKDATASIG fails if there are fewer than 3 items on stack.
		let too_few_args_sig_script = Builder::default()
			.push_data(&[1u8; 32])
			.push_data(&*message)
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();
		basic_test_with_flags(&too_few_args_sig_script, &correct_flags, Err(Error::InvalidStackOperation), vec![].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIG fails if <pubKey> is not a validly encoded public key.
		let incorrect_pubkey_script = Builder::default()
			.push_data(&*correct_signature)
			.push_data(&*message)
			.push_data(&[77u8; 15])
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();
		basic_test_with_flags(&incorrect_pubkey_script, &correct_flags, Err(Error::PubkeyType), vec![].into());

		// assuming that check_signature_encoding correctness is proved by other tests:
		// <sig> <msg> <pubKey> OP_CHECKDATASIG fails if <sig> is not a validly encoded signature with strict DER encoding.
		// <sig> <msg> <pubKey> OP_CHECKDATASIG fails if signature <sig> is not empty and does not pass the Low S check.
		let incorrectly_encoded_signature_script = Builder::default()
			.push_data(&[0u8; 65])
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();
		basic_test_with_flags(&incorrectly_encoded_signature_script, &correct_flags, Err(Error::SignatureDer), vec![].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIG fails if signature <sig> is not empty and does not pass signature validation of <msg> and <pubKey>.
		let incorrect_signature_script = Builder::default()
			.push_data(&*correct_signature_for_other_message)
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();
		basic_test_with_flags(&incorrect_signature_script, &correct_flags, Ok(false), vec![Bytes::new()].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIG pops three elements and pushes false onto the stack if <sig> is an empty byte array.
		let empty_signature_script = Builder::default()
			.push_data(&[])
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIG)
			.into_script();
		basic_test_with_flags(&empty_signature_script, &correct_flags, Ok(false), vec![Bytes::new()].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIG pops three elements and pushes true onto the stack if <sig> is a valid signature of <msg> with respect to <pubKey>.
		basic_test_with_flags(&correct_signature_script, &correct_flags, Ok(true), vec![vec![1].into()].into());
	}

	#[test]
	fn checkdatasigverify_spec_tests() {
		// official tests from:
		// https://github.com/bitcoincashorg/bitcoincash.org/blob/0c6f91b0b713aae3bc6c9834b46e80e247ff5fab/spec/op_checkdatasig.md

		let kp = KeyPair::from_private(Private { network: Network::Mainnet, secret: 1.into(), compressed: false, }).unwrap();

		let pubkey = kp.public().clone();
		let message = vec![42u8; 32];
		let correct_signature = kp.private().sign(&Message::from(sha256(&message))).unwrap();
		let correct_signature_for_other_message = kp.private().sign(&[43u8; 32].into()).unwrap();
		let mut correct_signature = correct_signature.to_vec();
		let mut correct_signature_for_other_message = correct_signature_for_other_message.to_vec();
		correct_signature.push(0x81);
		correct_signature_for_other_message.push(0x81);

		let correct_flags = VerificationFlags::default()
			.verify_checkdatasig(true)
			.verify_dersig(true)
			.verify_strictenc(true);
		let incorrect_flags = VerificationFlags::default().verify_checkdatasig(false);

		let correct_signature_script = Builder::default()
			.push_data(&*correct_signature)
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIGVERIFY)
			.into_script();

		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFY fails if 15 November 2018 protocol upgrade is not yet activated.
		basic_test_with_flags(&correct_signature_script, &incorrect_flags, Err(Error::DisabledOpcode(Opcode::OP_CHECKDATASIGVERIFY)), vec![].into());

		// <sig> <msg> OP_CHECKDATASIGVERIFY fails if there are fewer than 3 item on stack.
		let too_few_args_sig_script = Builder::default()
			.push_data(&[1u8; 32])
			.push_data(&*message)
			.push_opcode(Opcode::OP_CHECKDATASIGVERIFY)
			.into_script();
		basic_test_with_flags(&too_few_args_sig_script, &correct_flags, Err(Error::InvalidStackOperation), vec![].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFYfails if <pubKey> is not a validly encoded public key.
		let incorrect_pubkey_script = Builder::default()
			.push_data(&*correct_signature)
			.push_data(&*message)
			.push_data(&[77u8; 15])
			.push_opcode(Opcode::OP_CHECKDATASIGVERIFY)
			.into_script();
		basic_test_with_flags(&incorrect_pubkey_script, &correct_flags, Err(Error::PubkeyType), vec![].into());

		// assuming that check_signature_encoding correctness is proved by other tests:
		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFY fails if <sig> is not a validly encoded signature with strict DER encoding.
		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFY fails if signature <sig> is not empty and does not pass the Low S check.
		let incorrectly_encoded_signature_script = Builder::default()
			.push_data(&[0u8; 65])
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIGVERIFY)
			.into_script();
		basic_test_with_flags(&incorrectly_encoded_signature_script, &correct_flags, Err(Error::SignatureDer), vec![].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFY fails if <sig> is not a valid signature of <msg> with respect to <pubKey>.
		let incorrect_signature_script = Builder::default()
			.push_data(&*correct_signature_for_other_message)
			.push_data(&*message)
			.push_data(&*pubkey)
			.push_opcode(Opcode::OP_CHECKDATASIGVERIFY)
			.into_script();
		basic_test_with_flags(&incorrect_signature_script, &correct_flags, Err(Error::CheckDataSigVerify), vec![].into());

		// <sig> <msg> <pubKey> OP_CHECKDATASIGVERIFY pops the top three stack elements if <sig> is a valid signature of <msg> with respect to <pubKey>.
		// Ok(false) means success here, because OP_CHECKDATASIGVERIFY leaves empty stack
		basic_test_with_flags(&correct_signature_script, &correct_flags, Ok(false), vec![].into());
	}

	#[test]
	fn op_equal_push_empty_bytes_to_stack() {
		// tx #95 from testnet block:
		// https://testnet.blockexplorer.com/block/00000000c7169675fc165bfeceb11b572129977ca9f9e6ca5953e3184cb403dd
		// https://tbtc.bitaps.com/raw/transaction/27c94c0ca2f66fcc09d11b510e04d21adfe19f459673029e709024d7d9a7f4b4
		// and its donor tx:
		// https://tbtc.bitaps.com/raw/transaction/25e140942ecf79d24619908185a881f95d9ecb23a7be050f7c44cd378aae26eb
		//
		// the issue was that our comparison ops implementation (OP_EQUAL, OP_WITHIN, OP_CHECKSIG, ***)
		// were pushing non-empty value (vec![0]) when comparison has failed
		// => in combination with verify_nnulldummy this caused consensus issue

		let tx: Transaction = "0100000001eb26ae8a37cd447c0f05bea723cb9e5df981a88581901946d279cf2e9440e1250000000091473044022057e887c4cb773a6ec513b285dde1209ee4213209c21bb9da9e284ffe7477979302201aba367cf84bf2c6ccfd1b18d2bec0d705e2acacfeb42324cdc0fe63fbe2524a01483045022100e3f2e5e2a0b6bb75f2a506d7b190d8ba48b1e9108dd4fc4a740fbc921d0067a3022070fccd6eec2415d6d75f7aa3d0604988ee84d856db2acde4cc01d9c43f0237a301ffffffff0100350c00000000001976a9149e2be3b4d5e7274e8fd739b09fc6fd223054616088ac00000000".into();
		let signer: TransactionInputSigner = tx.into();
		let checker = TransactionSignatureChecker {
			signer: signer,
			input_index: 0,
			input_amount: 1000000,
		};
		let input: Script = "473044022057e887c4cb773a6ec513b285dde1209ee4213209c21bb9da9e284ffe7477979302201aba367cf84bf2c6ccfd1b18d2bec0d705e2acacfeb42324cdc0fe63fbe2524a01483045022100e3f2e5e2a0b6bb75f2a506d7b190d8ba48b1e9108dd4fc4a740fbc921d0067a3022070fccd6eec2415d6d75f7aa3d0604988ee84d856db2acde4cc01d9c43f0237a301".into();
		let output: Script = "5253877c5121027fe085933328a89d0ad069071dee3bd4c908fddc852032356a318324c9ab0f6c210321e7c9eea060c099747ddcf741e9498a2b90fe8f362e2c85370722df0f88d1782102a5bc779306b40927648e73e144d430dc1b7c0730f6a3ab5bbd130374d8fe4a5a53af2102a70faff961b367875336396076a72293bf3adaa084404f8a5cbec23f41645b87ac".into();
		let flags = VerificationFlags::default().verify_nulldummy(true);
		assert_eq!(verify_script(&input, &output, &ScriptWitness::default(), &flags, &checker, SignatureVersion::Base), Ok(()));
	}
}
