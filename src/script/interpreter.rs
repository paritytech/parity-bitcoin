use std::cmp;
use keys::{Public, Signature};
use hash::H256;
use transaction::{Transaction, SEQUENCE_LOCKTIME_DISABLE_FLAG};
use crypto::{sha1, sha256, dhash160, dhash256, ripemd160};
use script::{script, Script, Num, VerificationFlags, Opcode, Error, read_usize};

#[derive(Debug, PartialEq, Clone, Copy)]
#[repr(u8)]
pub enum SighashBase {
	All = 1,
	None = 2,
	Single = 3,
}


/// Documentation
/// https://en.bitcoin.it/wiki/OP_CHECKSIG#Procedure_for_Hashtype_SIGHASH_SINGLE
/// TODO: Possibly handle other integers when deserialing
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Sighash {
	pub base: SighashBase,
	pub anyone_can_pay: bool,
}

impl From<Sighash> for u32 {
	fn from(s: Sighash) -> Self {
		let base = s.base as u32;
		match s.anyone_can_pay {
			true => base | 0x80,
			false => base,
		}
	}
}

impl Sighash {
	fn from_u32(u: u32) -> Option<Self> {
		let (base, anyone_can_pay) = match u {
			1 => (SighashBase::All, false),
			2 => (SighashBase::None, false),
			3 => (SighashBase::Single, false),
			0x81 => (SighashBase::All, true),
			0x82 => (SighashBase::None, true),
			0x83 => (SighashBase::Single, true),
			_ => return None,
		};

		let sighash = Sighash {
			base: base,
			anyone_can_pay: anyone_can_pay
		};

		Some(sighash)
	}
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum SignatureVersion {
	Base,
	WitnessV0,
}

pub trait SignatureChecker {
	fn check_signature(
		&self,
		script_signature: &[u8],
		public: &[u8],
		script: &Script,
		version: SignatureVersion
	) -> bool;

	fn check_lock_time(&self, lock_time: Num) -> bool;

	fn check_sequence(&self, sequence: Num) -> bool;
}

pub struct NoopSignatureChecker;

impl SignatureChecker for NoopSignatureChecker {
	fn check_signature(&self, _: &[u8], _: &[u8], _: &Script, _: SignatureVersion) -> bool {
		false
	}

	fn check_lock_time(&self, _: Num) -> bool {
		false
	}

	fn check_sequence(&self, _: Num) -> bool {
		false
	}
}

pub struct TransactionSignatureChecker {
	transaction: Transaction,
	i: u32,
	amount: i64,
}

impl TransactionSignatureChecker {
	fn verify_signature(&self, _signature: &[u8], _public: &[u8], _hash: &H256) -> bool {
		unimplemented!();
	}
}

impl SignatureChecker for TransactionSignatureChecker {
	fn check_signature(
		&self,
		script_signature: &[u8],
		public: &[u8],
		_script: &Script,
		_version: SignatureVersion
	) -> bool {
		let public = match Public::from_slice(public) {
			Ok(public) => public,
			_ => return false,
		};

		if script_signature.is_empty() {
			return false;
		}

		let _hash_type = script_signature.last().unwrap();

		unimplemented!();
	}

	fn check_lock_time(&self, _lock_time: Num) -> bool {
		unimplemented!();
	}

	fn check_sequence(&self, _sequence: Num) -> bool {
		unimplemented!();
	}

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

	Sighash::from_u32(sig[sig.len() -1] as u32).is_some()
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
	if last == 0 || last == 0x80 {
		false
	} else {
		true
	}
}

#[inline]
fn require_not_empty(stack: &Vec<Vec<u8>>) -> Result<(), Error> {
	match stack.is_empty() {
		true => Err(Error::InvalidStackOperation),
		false => Ok(()),
	}
}

#[inline]
fn require_len(stack: &Vec<Vec<u8>>, len: usize) -> Result<(), Error> {
	match stack.len() < len {
		true => Err(Error::InvalidStackOperation),
		false => Ok(()),
	}
}

pub fn eval_script(
	stack: &mut Vec<Vec<u8>>,
	script: &Script,
	flags: &VerificationFlags,
	checker: &SignatureChecker,
	version: SignatureVersion
) -> Result<bool, Error> {
	if script.len() > script::MAX_SCRIPT_SIZE {
		return Err(Error::ScriptSize);
	}

	let mut pc = 0;
	let mut begincode = 0;
	let mut exec_stack = Vec::<bool>::new();
	let mut altstack = Vec::<Vec<u8>>::new();

	while pc < script.len() {
		let fexec = exec_stack.iter().find(|&x| !x).is_some();
		let opcode = try!(script.get_opcode(pc));
		match opcode {
			Opcode::OP_PUSHDATA1 |
			Opcode::OP_PUSHDATA2 |
			Opcode::OP_PUSHDATA4 => {
				let len = match opcode {
					Opcode::OP_PUSHDATA1 => 1,
					Opcode::OP_PUSHDATA2 => 2,
					_ => 4,
				};

				let slice = try!(script.take(pc + 1, len));
				let n = try!(read_usize(slice, len));
				let bytes = try!(script.take_checked(pc + 1 + len, n));
				if flags.verify_minimaldata && !check_minimal_push(bytes, opcode) {
					return Err(Error::Minimaldata);
				}
				stack.push(bytes.to_vec());
				pc += len + n;
			},
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
				let bytes = try!(script.take_checked(pc + 1, opcode as usize));
				if flags.verify_minimaldata && !check_minimal_push(bytes, opcode) {
					return Err(Error::Minimaldata);
				}
				stack.push(bytes.to_vec());
				pc += opcode as usize;
			},
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
				let value = opcode as u8 - (Opcode::OP_1 as u8 - 1);
				stack.push(Num::from(value).to_vec());
			},
			Opcode::OP_CAT | Opcode::OP_SUBSTR | Opcode::OP_LEFT | Opcode::OP_RIGHT |
			Opcode::OP_INVERT | Opcode::OP_AND | Opcode::OP_OR | Opcode::OP_XOR |
			Opcode::OP_2MUL | Opcode::OP_2DIV | Opcode::OP_MUL | Opcode::OP_DIV |
			Opcode::OP_MOD | Opcode::OP_LSHIFT | Opcode::OP_RSHIFT => {
				return Err(Error::DisabledOpcode(opcode));
			},
			Opcode::OP_NOP => break,
			Opcode::OP_CHECKLOCKTIMEVERIFY => {
				if !flags.verify_clocktimeverify {
					if flags.verify_discourage_upgradable_nops {
						return Err(Error::DiscourageUpgradableNops);
					}
				}

				try!(require_not_empty(stack));

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
				let lock_time = try!(Num::from_slice(stack.last().unwrap(), flags.verify_minimaldata, 5));

				// In the rare event that the argument may be < 0 due to
				// some arithmetic being done first, you can always use
				// 0 MAX CHECKLOCKTIMEVERIFY.
				if lock_time.is_negative() {
					return Err(Error::NegativeLocktime);
				}

				if !checker.check_lock_time(lock_time) {
					return Err(Error::UnsatisfiedLocktime);
				}
			},
			Opcode::OP_CHECKSEQUENCEVERIFY => {
				if !flags.verify_chechsequenceverify {
					if flags.verify_discourage_upgradable_nops {
						return Err(Error::DiscourageUpgradableNops);
					}
				}

				try!(require_not_empty(stack));

				let sequence = try!(Num::from_slice(stack.last().unwrap(), flags.verify_minimaldata, 5));

				if sequence.is_negative() {
					return Err(Error::NegativeLocktime);
				}

				if (sequence & (SEQUENCE_LOCKTIME_DISABLE_FLAG as i64).into()).is_zero() {
					if !checker.check_sequence(sequence) {
						return Err(Error::UnsatisfiedLocktime);
					}
				}
			},
			Opcode::OP_NOP1 | Opcode::OP_NOP4 | Opcode::OP_NOP5 | Opcode::OP_NOP6 |
				Opcode::OP_NOP7 | Opcode::OP_NOP8 | Opcode::OP_NOP9 | Opcode::OP_NOP10 => {
				if flags.verify_discourage_upgradable_nops {
					return Err(Error::DiscourageUpgradableNops);
				}
			},
			Opcode::OP_IF | Opcode::OP_NOTIF => {
				let mut exec_value = false;
				if fexec {
					try!(require_not_empty(stack).map_err(|_| Error::UnbalancedConditional));
					exec_value = cast_to_bool(&stack.pop().unwrap());
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
				let last = exec_stack[exec_stack.len() - 1];
				exec_stack[exec_stack.len() - 1] == !last;
			},
			Opcode::OP_ENDIF => {
				if exec_stack.is_empty() {
					return Err(Error::UnbalancedConditional);
				}
				exec_stack.pop();
			},
			Opcode::OP_VERIFY => {
				try!(require_not_empty(stack));
				// should we return an error without popping the value?
				let exec_value = cast_to_bool(&stack.pop().unwrap());
				if !exec_value {
					return Err(Error::Verify);
				}
			},
			Opcode::OP_RETURN => {
				return Err(Error::ReturnOpcode);
			},
			Opcode::OP_TOALTSTACK => {
				try!(require_not_empty(stack));
				altstack.push(stack.pop().unwrap());
			},
			Opcode::OP_FROMALTSTACK => {
				try!(require_not_empty(&altstack).map_err(|_| Error::InvalidAltstackOperation));
				stack.push(altstack.pop().unwrap());
			},
			Opcode::OP_2DROP => {
				try!(require_len(stack, 2));
				stack.pop();
				stack.pop();
			},
			Opcode::OP_2DUP => {
				try!(require_len(stack, 2));
				let v1 = stack[stack.len() - 2].clone();
				let v2 = stack[stack.len() - 1].clone();
				stack.push(v1);
				stack.push(v2);
			},
			Opcode::OP_3DUP => {
				try!(require_len(stack, 3));
				let v1 = stack[stack.len() - 3].clone();
				let v2 = stack[stack.len() - 2].clone();
				let v3 = stack[stack.len() - 1].clone();
				stack.push(v1);
				stack.push(v2);
				stack.push(v3);
			},
			Opcode::OP_2OVER => {
				try!(require_len(stack, 4));
				let v1 = stack[stack.len() - 4].clone();
				let v2 = stack[stack.len() - 3].clone();
				stack.push(v1);
				stack.push(v2);
			},
			Opcode::OP_2ROT => {
				try!(require_len(stack, 6));
				let v1 = stack[stack.len() - 6].clone();
				let v2 = stack[stack.len() - 5].clone();
				let len = stack.len();
				stack.remove(len - 6);
				// -5 -just removed element
				stack.remove(len - 6);
				stack.push(v1);
				stack.push(v2);
			},
			Opcode::OP_2SWAP => {
				try!(require_len(stack, 4));
				let len = stack.len();
				stack.swap(len - 4, len - 2);
				stack.swap(len - 3, len - 1);
			},
			Opcode::OP_IFDUP => {
				try!(require_not_empty(stack));
				if cast_to_bool(stack.last().unwrap()) {
					let last = stack.last().unwrap().clone();
					stack.push(last);
				}
			},
			Opcode::OP_DEPTH => {
				let depth = Num::from(stack.len());
				stack.push(depth.to_vec());
			},
			Opcode::OP_DROP => {
				try!(require_not_empty(stack));
				stack.pop();
			},
			Opcode::OP_DUP => {
				try!(require_not_empty(stack));
				let v1 = stack[stack.len() - 1].clone();
				stack.push(v1);
			},
			Opcode::OP_NIP => {
				try!(require_len(stack, 2));
				let len = stack.len();
				stack.swap_remove(len - 2);
			},
			Opcode::OP_OVER => {
				try!(require_len(stack, 2));
				let v = stack[stack.len() - 2].clone();
				stack.push(v);
			},
			Opcode::OP_PICK | Opcode::OP_ROLL => {
				try!(require_len(stack, 2));
				let n: i64 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)).into();
				if n < 0 || n >= stack.len() as i64 {
					return Err(Error::InvalidStackOperation);
				}

				let v = stack[n as usize + 1].clone();
				if opcode == Opcode::OP_ROLL {
					stack.remove(n as usize + 1);
				}
				stack.push(v);
			},
			Opcode::OP_ROT => {
				try!(require_len(stack, 3));
				let len = stack.len();
				stack.swap(len - 3, len - 2);
				stack.swap(len - 2, len - 1);
			},
			Opcode::OP_SWAP => {
				try!(require_len(stack, 2));
				let len = stack.len();
				stack.swap(len - 2, len - 1);
			},
			Opcode::OP_TUCK => {
				try!(require_len(stack, 2));
				let len = stack.len();
				let v = stack[len - 1].clone();
				stack.insert(len - 2, v);
			},
			Opcode::OP_SIZE => {
				try!(require_not_empty(stack));
				let n = Num::from(stack.last().unwrap().len());
				stack.push(n.to_vec());
			},
			Opcode::OP_EQUAL => {
				try!(require_len(stack, 2));
				let v1 = stack.pop();
				let v2 = stack.pop();
				let to_push = match v1 == v2 {
					true => vec![1],
					false => vec![0],
				};
				stack.push(to_push);
			},
			Opcode::OP_EQUALVERIFY => {
				try!(require_len(stack, 2));
				let equal = stack.pop() == stack.pop();
				if !equal {
					return Err(Error::EqualVerify);
				}
			},
			Opcode::OP_1ADD => {
				try!(require_not_empty(stack));
				let n = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)) + 1.into();
				stack.push(n.to_vec());
			},
			Opcode::OP_1SUB => {
				try!(require_not_empty(stack));
				let n = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)) - 1.into();
				stack.push(n.to_vec());
			},
			Opcode::OP_NEGATE => {
				try!(require_not_empty(stack));
				let n = -try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				stack.push(n.to_vec());
			},
			Opcode::OP_ABS => {
				try!(require_not_empty(stack));
				let n = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)).abs();
				stack.push(n.to_vec());
			},
			Opcode::OP_NOT => {
				try!(require_not_empty(stack));
				let n = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)).is_zero();
				let n = Num::from(n);
				stack.push(n.to_vec());
			},
			Opcode::OP_0NOTEQUAL => {
				try!(require_not_empty(stack));
				let n = !try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4)).is_zero();
				let n = Num::from(n);
				stack.push(n.to_vec());
			},
			Opcode::OP_ADD => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				stack.push((v1 + v2).to_vec());
			},
			Opcode::OP_SUB => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				stack.push((v2 - v1).to_vec());
			},
			Opcode::OP_BOOLAND => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(!v1.is_zero() && !v2.is_zero());
				stack.push(v.to_vec());
			},
			Opcode::OP_BOOLOR => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(!v1.is_zero() || !v2.is_zero());
				stack.push(v.to_vec());
			},
			Opcode::OP_NUMEQUAL => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 == v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_NUMEQUALVERIFY => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				if v1 != v2 {
					return Err(Error::NumEqualVerify);
				}
			},
			Opcode::OP_NUMNOTEQUAL => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 != v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_LESSTHAN => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 > v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_GREATERTHAN => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 < v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_LESSTHANOREQUAL => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 >= v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_GREATERTHANOREQUAL => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v = Num::from(v1 <= v2);
				stack.push(v.to_vec());
			},
			Opcode::OP_MIN => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				stack.push(cmp::min(v1, v2).to_vec());
			},
			Opcode::OP_MAX => {
				try!(require_len(stack, 2));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				stack.push(cmp::max(v1, v2).to_vec());
			},
			Opcode::OP_WITHIN => {
				try!(require_len(stack, 3));
				let v1 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v2 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let v3 = try!(Num::from_slice(&stack.pop().unwrap(), flags.verify_minimaldata, 4));
				let to_push = match v2 <= v3 && v3 <= v1 {
					true => vec![1],
					false => vec![0],
				};
				stack.push(to_push);
			},
			Opcode::OP_RIPEMD160 => {
				try!(require_not_empty(stack));
				let v = ripemd160(&stack.pop().unwrap());
				stack.push(v.to_vec());
			},
			Opcode::OP_SHA1 => {
				try!(require_not_empty(stack));
				let v = sha1(&stack.pop().unwrap());
				stack.push(v.to_vec());
			},
			Opcode::OP_SHA256 => {
				try!(require_not_empty(stack));
				let v = sha256(&stack.pop().unwrap());
				stack.push(v.to_vec());
			},
			Opcode::OP_HASH160 => {
				try!(require_not_empty(stack));
				let v = dhash160(&stack.pop().unwrap());
				stack.push(v.to_vec());
			},
			Opcode::OP_HASH256 => {
				try!(require_not_empty(stack));
				let v = dhash256(&stack.pop().unwrap());
				stack.push(v.to_vec());
			},
			Opcode::OP_CODESEPARATOR => {
				begincode = pc;
			},
			Opcode::OP_CHECKSIG | Opcode::OP_CHECKSIGVERIFY => {
				try!(require_len(stack, 2));
				let pubkey = stack.pop().unwrap();
				let signature = stack.pop().unwrap();
				let mut subscript = script.subscript(begincode);
				if version == SignatureVersion::Base {
					subscript = script.find_and_delete(&signature);
				}

				try!(check_signature_encoding(&signature, flags));
				try!(check_pubkey_encoding(&pubkey, flags));

				let success = checker.check_signature(&signature, &pubkey, &subscript, version);
				match opcode {
					Opcode::OP_CHECKSIG => {
						let to_push = match success {
							true => vec![1],
							false => vec![0],
						};
						stack.push(to_push);
					},
					Opcode::OP_CHECKSIGVERIFY if !success => {
						return Err(Error::CheckSigVerify);
					},
					_ => {},
				}
			},
			_ => {},
		}
		pc += 1;
	}

	let success = !stack.is_empty() && {
		let last = stack.last().unwrap();
		last != &vec![0; last.len()]
	};

	Ok(success)
}

#[cfg(test)]
mod tests {
	use hex::FromHex;
	use script::{Opcode, Script, VerificationFlags, Builder, Error, Num};
	use super::{is_public_key, eval_script, NoopSignatureChecker, SignatureVersion};

	#[test]
	fn tests_is_public_key() {
		assert!(!is_public_key(&[]));
		assert!(!is_public_key(&[1]));
		assert!(is_public_key(&"0495dfb90f202c7d016ef42c65bc010cd26bb8237b06253cc4d12175097bef767ed6b1fcb3caf1ed57c98d92e6cb70278721b952e29a335134857acd4c199b9d2f".from_hex().unwrap()));
		assert!(is_public_key(&[2; 33]));
		assert!(is_public_key(&[3; 33]));
		assert!(!is_public_key(&[4; 33]));
	}

	// https://github.com/bitcoin/bitcoin/blob/d612837814020ae832499d18e6ee5eb919a87907/src/test/script_tests.cpp#L900
	#[test]
	fn test_push_data() {
		let expected = vec![vec![0x5a]];
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		let checker = NoopSignatureChecker;
		let version = SignatureVersion::Base;
		let direct = Script::new(vec![Opcode::OP_PUSHBYTES_1 as u8, 0x5a]);
		let pushdata1 = Script::new(vec![Opcode::OP_PUSHDATA1 as u8, 0x1, 0x5a]);
		let pushdata2 = Script::new(vec![Opcode::OP_PUSHDATA2 as u8, 0x1, 0, 0x5a]);
		let pushdata4 = Script::new(vec![Opcode::OP_PUSHDATA4 as u8, 0x1, 0, 0, 0, 0x5a]);

		let mut direct_stack = vec![];
		let mut pushdata1_stack= vec![];
		let mut pushdata2_stack= vec![];
		let mut pushdata4_stack= vec![];
		assert!(eval_script(&mut direct_stack, &direct, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata1_stack, &pushdata1, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata2_stack, &pushdata2, &flags, &checker, version).unwrap());
		assert!(eval_script(&mut pushdata4_stack, &pushdata4, &flags, &checker, version).unwrap());

		assert_eq!(expected, direct_stack);
		assert_eq!(expected, pushdata1_stack);
		assert_eq!(expected, pushdata2_stack);
		assert_eq!(expected, pushdata4_stack);
	}

	fn basic_test(script: &Script, expected: Result<bool, Error>, expected_stack: Vec<Vec<u8>>) {
		let flags = VerificationFlags::default()
			.verify_p2sh(true);
		let checker = NoopSignatureChecker;
		let version = SignatureVersion::Base;
		let mut stack = vec![];
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
		let stack = vec![vec![1]];
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
		let stack = vec![vec![0]];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_equal_invalid_stack() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_equal_verify() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUALVERIFY)
			.into_script();
		let result = Ok(false);
		let stack = vec![];
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
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_equal_verify_invalid_stack() {
		let script = Builder::default()
			.push_data(&[0x4])
			.push_opcode(Opcode::OP_EQUALVERIFY)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_size() {
		let script = Builder::default()
			.push_data(&[0x12, 0x34])
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Ok(true);
		let stack = vec![vec![0x12, 0x34], vec![0x2]];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_size_false() {
		let script = Builder::default()
			.push_data(&[])
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Ok(false);
		let stack = vec![vec![], vec![]];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_size_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SIZE)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_hash256() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_HASH256)
			.into_script();
		let result = Ok(true);
		let stack = vec!["9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".from_hex().unwrap()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_hash256_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_HASH256)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_ripemd160() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_RIPEMD160)
			.into_script();
		let result = Ok(true);
		let stack = vec!["108f07b8382412612c048d07d13f814118445acd".from_hex().unwrap()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_ripemd160_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_RIPEMD160)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_sha1() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_SHA1)
			.into_script();
		let result = Ok(true);
		let stack = vec!["aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".from_hex().unwrap()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sha1_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SHA1)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_sha256() {
		let script = Builder::default()
			.push_data(b"hello")
			.push_opcode(Opcode::OP_SHA256)
			.into_script();
		let result = Ok(true);
		let stack = vec!["2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".from_hex().unwrap()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sha256_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_SHA256)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_1add() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1ADD)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(6).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_1add_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_1ADD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_1sub() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_1SUB)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(4).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_1sub_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_1SUB)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_negate() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(-5).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_negate_negative() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_negate_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_NEGATE)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_abs() {
		let script = Builder::default()
			.push_num(5.into())
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_abs_negative() {
		let script = Builder::default()
			.push_num((-5).into())
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_abs_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_ABS)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_not() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_not_zero() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_not_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_NOT)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_0notequal() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_0notequal_zero() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Ok(false);
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_0notequal_invalid_stack() {
		let script = Builder::default()
			.push_opcode(Opcode::OP_0NOTEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_add() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(5).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_add_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_sub() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUB)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_sub_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_SUB)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_booland() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_booland_invalid_stack() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLAND)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_boolor() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_boolor_invalid_stack() {
		let script = Builder::default()
			.push_num(0.into())
			.push_opcode(Opcode::OP_BOOLOR)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_numequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_numequalverify() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Ok(false);
		let stack = vec![];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numequalverify_failed() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Err(Error::NumEqualVerify);
		let stack = vec![];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numequalverify_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMEQUALVERIFY)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		let stack = vec![];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numnotequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_NUMNOTEQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_numnotequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_NUMNOTEQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_lessthan() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_LESSTHAN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthan_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHAN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_greaterthan() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHAN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthan_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHAN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_lessthanorequal() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_lessthanorequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_LESSTHANOREQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_greaterthanorequal() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(1).to_vec()];
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
		let stack = vec![Num::from(0).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_greaterthanorequal_invalid_stack() {
		let script = Builder::default()
			.push_num(2.into())
			.push_opcode(Opcode::OP_GREATERTHANOREQUAL)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_min() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MIN)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(2).to_vec()];
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
		let stack = vec![Num::from(3).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_min_invalid_stack() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_MIN)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
	}

	#[test]
	fn test_max() {
		let script = Builder::default()
			.push_num(2.into())
			.push_num(3.into())
			.push_opcode(Opcode::OP_MAX)
			.into_script();
		let result = Ok(true);
		let stack = vec![Num::from(3).to_vec()];
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
		let stack = vec![Num::from(4).to_vec()];
		basic_test(&script, result, stack);
	}

	#[test]
	fn test_max_invalid_stack() {
		let script = Builder::default()
			.push_num(4.into())
			.push_opcode(Opcode::OP_MAX)
			.into_script();
		let result = Err(Error::InvalidStackOperation);
		basic_test(&script, result, vec![]);
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
		let stack = vec![vec![1]];
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
		let stack = vec![vec![0]];
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
		basic_test(&script, result, vec![]);
	}
}
