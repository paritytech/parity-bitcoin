use std::{cmp, mem};
use bytes::Bytes;
use keys::{Signature, Public};
use chain::constants::SEQUENCE_LOCKTIME_DISABLE_FLAG;
use crypto::{sha1, sha256, dhash160, dhash256, ripemd160};
use sign::{SignatureVersion, Sighash};
use {
	script, Builder, Script, Num, VerificationFlags, Opcode, Error, SignatureChecker, Stack
};

/// Helper function.
fn check_signature(
	checker: &SignatureChecker,
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
	if len_r > 1 && sig[4] == 0 && (!(sig[5] & 0x80)) != 0 {
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
	if len_s > 1 && (sig[len_r + 6] == 0) && (!(sig[len_r + 7] & 0x80)) != 0 {
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

fn is_defined_hashtype_signature(sig: &[u8]) -> bool {
	if sig.is_empty() {
		return false;
	}

	Sighash::is_defined(sig[sig.len() -1] as u32)
}

fn check_signature_encoding(sig: &[u8], flags: &VerificationFlags) -> Result<(), Error> {
	// Empty signature. Not strictly DER encoded, but allowed to provide a
	// compact way to provide an invalid signature for use with CHECK(MULTI)SIG

	if sig.is_empty() {
		return Ok(());
	}

	if (flags.verify_dersig || flags.verify_low_s || flags.verify_strictenc) && !is_valid_signature_encoding(sig) {
		return Err(Error::SignatureDer);
	}

	if flags.verify_low_s {
		try!(is_low_der_signature(sig));
	}

	if flags.verify_strictenc && !is_defined_hashtype_signature(sig) {
		Err(Error::SignatureHashtype)
	} else {
		Ok(())
	}
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
	flags: &VerificationFlags,
	checker: &SignatureChecker,
	version: SignatureVersion,
) -> Result<(), Error> {
	if flags.verify_sigpushonly && !script_sig.is_push_only() {
		return Err(Error::SignaturePushOnly);
	}

	let mut stack = Stack::new();
	let mut stack_copy = Stack::new();

	try!(eval_script(&mut stack, script_sig, flags, checker, version));

	if flags.verify_p2sh {
		stack_copy = stack.clone();
	}

	let res = try!(eval_script(&mut stack, script_pubkey, flags, checker, version));
	if !res {
		return Err(Error::EvalFalse);
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

		let pubkey2: Script = try!(stack.pop()).into();

		let res = try!(eval_script(&mut stack, &pubkey2, flags, checker, version));
		if !res {
			return Err(Error::EvalFalse);
		}
	}

    // The CLEANSTACK check is only performed after potential P2SH evaluation,
    // as the non-P2SH evaluation of a P2SH script will obviously not result in
    // a clean stack (the P2SH inputs remain). The same holds for witness evaluation.
	if flags.verify_cleanstack {
        // Disallow CLEANSTACK without P2SH, as otherwise a switch CLEANSTACK->P2SH+CLEANSTACK
        // would be possible, which is not a softfork (and P2SH should be one).
		assert!(flags.verify_p2sh);
		assert!(flags.verify_witness);
		if stack.len() != 1 {
			return Err(Error::Cleanstack);
		}
	}

	Ok(())
}

/// Evaluautes the script
#[cfg_attr(feature="cargo-clippy", allow(match_same_arms))]
pub fn eval_script(
	stack: &mut Stack<Bytes>,
	script: &Script,
	flags: &VerificationFlags,
	checker: &SignatureChecker,
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

		if opcode.is_disabled() {
			return Err(Error::DisabledOpcode(opcode));
		}

		if !(executing || (Opcode::OP_IF <= opcode && opcode <= Opcode::OP_ENDIF)) {
			pc += instruction.step;
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
			Opcode::OP_CAT | Opcode::OP_SUBSTR | Opcode::OP_LEFT | Opcode::OP_RIGHT |
			Opcode::OP_INVERT | Opcode::OP_AND | Opcode::OP_OR | Opcode::OP_XOR |
			Opcode::OP_2MUL | Opcode::OP_2DIV | Opcode::OP_MUL | Opcode::OP_DIV |
			Opcode::OP_MOD | Opcode::OP_LSHIFT | Opcode::OP_RSHIFT => {
				return Err(Error::DisabledOpcode(opcode));
			},
			Opcode::OP_NOP => break,
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
					let lock_time = try!(Num::from_slice(try!(stack.last()), flags.verify_minimaldata, 5));

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
					let sequence = try!(Num::from_slice(try!(stack.last()), flags.verify_minimaldata, 5));

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
					exec_value = cast_to_bool(&try!(stack.pop().map_err(|_| Error::UnbalancedConditional)));
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
				let exec_value = cast_to_bool(&try!(stack.pop()));
				if !exec_value {
					return Err(Error::Verify);
				}
			},
			Opcode::OP_RETURN => {
				return Err(Error::ReturnOpcode);
			},
			Opcode::OP_TOALTSTACK => {
				altstack.push(try!(stack.pop()));
			},
			Opcode::OP_FROMALTSTACK => {
				stack.push(try!(altstack.pop().map_err(|_| Error::InvalidAltstackOperation)));
			},
			Opcode::OP_2DROP => {
				try!(stack.drop(2));
			},
			Opcode::OP_2DUP => {
				try!(stack.dup(2));
			},
			Opcode::OP_3DUP => {
				try!(stack.dup(3));
			},
			Opcode::OP_2OVER => {
				try!(stack.over(2));
			},
			Opcode::OP_2ROT => {
				try!(stack.rot(2));
			},
			Opcode::OP_2SWAP => {
				try!(stack.swap(2));
			},
			Opcode::OP_IFDUP => {
				if cast_to_bool(try!(stack.last())) {
					try!(stack.dup(1));
				}
			},
			Opcode::OP_DEPTH => {
				let depth = Num::from(stack.len());
				stack.push(depth.to_bytes());
			},
			Opcode::OP_DROP => {
				try!(stack.pop());
			},
			Opcode::OP_DUP => {
				try!(stack.dup(1));
			},
			Opcode::OP_NIP => {
				try!(stack.nip());
			},
			Opcode::OP_OVER => {
				try!(stack.over(1));
			},
			Opcode::OP_PICK | Opcode::OP_ROLL => {
				let n: i64 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)).into();
				if n < 0 || n >= stack.len() as i64 {
					return Err(Error::InvalidStackOperation);
				}

				let v = match opcode {
					Opcode::OP_PICK => try!(stack.top(n as usize)).clone(),
					_ => try!(stack.remove(n as usize)),
				};

				stack.push(v);
			},
			Opcode::OP_ROT => {
				try!(stack.rot(1));
			},
			Opcode::OP_SWAP => {
				try!(stack.swap(1));
			},
			Opcode::OP_TUCK => {
				try!(stack.tuck());
			},
			Opcode::OP_SIZE => {
				let n = Num::from(try!(stack.last()).len());
				stack.push(n.to_bytes());
			},
			Opcode::OP_EQUAL => {
				let v1 = try!(stack.pop());
				let v2 = try!(stack.pop());
				if v1 == v2 {
					stack.push(vec![1].into());
				} else {
					stack.push(vec![0].into());
				}
			},
			Opcode::OP_EQUALVERIFY => {
				let equal = try!(stack.pop()) == try!(stack.pop());
				if !equal {
					return Err(Error::EqualVerify);
				}
			},
			Opcode::OP_1ADD => {
				let n = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)) + 1.into();
				stack.push(n.to_bytes());
			},
			Opcode::OP_1SUB => {
				let n = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)) - 1.into();
				stack.push(n.to_bytes());
			},
			Opcode::OP_NEGATE => {
				let n = -try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				stack.push(n.to_bytes());
			},
			Opcode::OP_ABS => {
				let n = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)).abs();
				stack.push(n.to_bytes());
			},
			Opcode::OP_NOT => {
				let n = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)).is_zero();
				let n = Num::from(n);
				stack.push(n.to_bytes());
			},
			Opcode::OP_0NOTEQUAL => {
				let n = !try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4)).is_zero();
				let n = Num::from(n);
				stack.push(n.to_bytes());
			},
			Opcode::OP_ADD => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				stack.push((v1 + v2).to_bytes());
			},
			Opcode::OP_SUB => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				stack.push((v2 - v1).to_bytes());
			},
			Opcode::OP_BOOLAND => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(!v1.is_zero() && !v2.is_zero());
				stack.push(v.to_bytes());
			},
			Opcode::OP_BOOLOR => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(!v1.is_zero() || !v2.is_zero());
				stack.push(v.to_bytes());
			},
			Opcode::OP_NUMEQUAL => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 == v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_NUMEQUALVERIFY => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				if v1 != v2 {
					return Err(Error::NumEqualVerify);
				}
			},
			Opcode::OP_NUMNOTEQUAL => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 != v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_LESSTHAN => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 > v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_GREATERTHAN => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 < v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_LESSTHANOREQUAL => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 >= v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_GREATERTHANOREQUAL => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v = Num::from(v1 <= v2);
				stack.push(v.to_bytes());
			},
			Opcode::OP_MIN => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				stack.push(cmp::min(v1, v2).to_bytes());
			},
			Opcode::OP_MAX => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				stack.push(cmp::max(v1, v2).to_bytes());
			},
			Opcode::OP_WITHIN => {
				let v1 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				let v3 = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				if v2 <= v3 && v3 < v1 {
					stack.push(vec![1].into());
				} else {
					stack.push(vec![0].into());
				}
			},
			Opcode::OP_RIPEMD160 => {
				let v = ripemd160(&try!(stack.pop()));
				stack.push(v.to_vec().into());
			},
			Opcode::OP_SHA1 => {
				let v = sha1(&try!(stack.pop()));
				stack.push(v.to_vec().into());
			},
			Opcode::OP_SHA256 => {
				let v = sha256(&try!(stack.pop()));
				stack.push(v.to_vec().into());
			},
			Opcode::OP_HASH160 => {
				let v = dhash160(&try!(stack.pop()));
				stack.push(v.to_vec().into());
			},
			Opcode::OP_HASH256 => {
				let v = dhash256(&try!(stack.pop()));
				stack.push(v.to_vec().into());
			},
			Opcode::OP_CODESEPARATOR => {
				begincode = pc;
			},
			Opcode::OP_CHECKSIG | Opcode::OP_CHECKSIGVERIFY => {
				let pubkey = try!(stack.pop());
				let signature = try!(stack.pop());
				let mut subscript = script.subscript(begincode);
				if version == SignatureVersion::Base {
					let signature_script = Builder::default().push_data(&*signature).into_script();
					subscript = subscript.find_and_delete(&*signature_script);
				}

				try!(check_signature_encoding(&signature, flags));
				try!(check_pubkey_encoding(&pubkey, flags));

				let success = check_signature(checker, signature.into(), pubkey.into(), &subscript, version);
				match opcode {
					Opcode::OP_CHECKSIG => {
						if success {
							stack.push(vec![1].into());
						} else {
							stack.push(vec![0].into());
						}
					},
					Opcode::OP_CHECKSIGVERIFY if !success => {
						return Err(Error::CheckSigVerify);
					},
					_ => {},
				}
			},
			Opcode::OP_CHECKMULTISIG | Opcode::OP_CHECKMULTISIGVERIFY => {
				let keys_count = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				if keys_count < 0.into() || keys_count > script::MAX_PUBKEYS_PER_MULTISIG.into() {
					return Err(Error::PubkeyCount);
				}

				let keys_count: usize = keys_count.into();
				let keys: Vec<_> = try!((0..keys_count).into_iter().map(|_| stack.pop()).collect());

				let sigs_count = try!(Num::from_slice(&try!(stack.pop()), flags.verify_minimaldata, 4));
				if sigs_count < 0.into() || sigs_count > keys_count.into() {
					return Err(Error::SigCount);
				}

				let sigs_count: usize = sigs_count.into();
				let sigs: Vec<_> = try!((0..sigs_count).into_iter().map(|_| stack.pop()).collect());

				let mut subscript = script.subscript(begincode);

				if version == SignatureVersion::Base {
					for signature in &sigs {
						let signature_script = Builder::default().push_data(&*signature).into_script();
						subscript = subscript.find_and_delete(&*signature_script);
					}
				}

				let mut success = true;
				let mut k = 0;
				let mut s = 0;
				while s < sigs.len() && success {
					// TODO: remove redundant copying
					let key = keys[k].clone();
					let sig = sigs[s].clone();

					try!(check_signature_encoding(&sig, flags));
					try!(check_pubkey_encoding(&key, flags));

					let ok = check_signature(checker, sig.into(), key.into(), &subscript, version);
					if ok {
						s += 1;
					}
					k += 1;

					success = sigs.len() - s <= keys.len() - k;
				}

				if !try!(stack.pop()).is_empty() && flags.verify_nulldummy {
					return Err(Error::SignatureNullDummy);
				}

				match opcode {
					Opcode::OP_CHECKMULTISIG => {
						if success {
							stack.push(vec![1].into());
						} else {
							stack.push(vec![0].into());
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
		}

		if stack.len() + altstack.len() > 1000 {
			return Err(Error::StackSize);
		}

		pc += instruction.step;
	}

	if !exec_stack.is_empty() {
		return Err(Error::UnbalancedConditional);
	}

	let success = !stack.is_empty() && {
		let last = try!(stack.last());
		cast_to_bool(last)
	};

	Ok(success)
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use chain::Transaction;
	use sign::SignatureVersion;
	use {
		Opcode, Script, VerificationFlags, Builder, Error, Num, TransactionInputSigner,
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

	fn basic_test(script: &Script, expected: Result<bool, Error>, expected_stack: Stack<Bytes>) {
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		let checker = NoopSignatureChecker;
		let version = SignatureVersion::Base;
		let mut stack = Stack::new();
		assert_eq!(eval_script(&mut stack, script, &flags, &checker, version), expected);
		if expected.is_ok() {
			assert_eq!(stack, expected_stack);
		}
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
		let stack = vec![vec![0].into()].into();
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
		let stack = vec![vec![0].into()].into();
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));

		let flags = VerificationFlags::default()
			.verify_p2sh(true)
			.verify_locktime(true);
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Err(Error::NumberOverflow));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
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
		assert_eq!(verify_script(&input, &output, &flags, &checker, SignatureVersion::Base), Ok(()));
	}
}

