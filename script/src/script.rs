//! Serialized script, used inside transaction inputs and outputs.

use std::{fmt, ops};
use bytes::Bytes;
use {Opcode, Error};

/// Maximum number of bytes pushable to the stack
pub const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;

/// Maximum number of non-push operations per script
pub const MAX_OPS_PER_SCRIPT: u32 = 201;

/// Maximum number of public keys per multisig
pub const MAX_PUBKEYS_PER_MULTISIG: usize = 20;

/// Maximum script length in bytes
pub const MAX_SCRIPT_SIZE: usize = 10000;

/// Threshold for `nLockTime`: below this value it is interpreted as block number,
/// otherwise as UNIX timestamp.
pub const LOCKTIME_THRESHOLD: u32 = 500000000; // Tue Nov  5 00:53:20 1985 UTC

#[derive(PartialEq, Debug)]
pub enum ScriptType {
	NonStandard,
	PubKey,
	PubKeyHash,
	ScriptHash,
	Multisig,
	NullData,
}

/// Serialized script, used inside transaction inputs and outputs.
#[derive(PartialEq, Debug)]
pub struct Script {
	data: Bytes,
}

impl From<&'static str> for Script {
	fn from(s: &'static str) -> Self {
		Script::new(s.into())
	}
}

impl From<Bytes> for Script {
	fn from(s: Bytes) -> Self {
		Script::new(s)
	}
}

impl From<Vec<u8>> for Script {
	fn from(v: Vec<u8>) -> Self {
		Script::new(v.into())
	}
}

impl Script {
	/// Script constructor.
	pub fn new(data: Bytes) -> Self {
		Script {
			data: data,
		}
	}

	pub fn to_bytes(&self) -> Bytes {
		self.data.clone()
	}

	/// Extra-fast test for pay-to-public-key-hash (P2PKH) scripts.
	pub fn is_pay_to_public_key_hash(&self) -> bool {
		self.data.len() == 25 &&
			self.data[0] == Opcode::OP_DUP as u8 &&
			self.data[1] == Opcode::OP_HASH160 as u8 &&
			self.data[2] == Opcode::OP_PUSHBYTES_20 as u8 &&
			self.data[23] == Opcode::OP_EQUALVERIFY as u8 &&
			self.data[24] == Opcode::OP_CHECKSIG as u8
	}

	/// Extra-fast test for pay-to-public-key (P2PK) scripts.
	pub fn is_pay_to_public_key(&self) -> bool {
		if self.data.is_empty() {
			return false;
		}

		let len = match self.data[0] {
			x if x == Opcode::OP_PUSHBYTES_33 as u8 => 35,
			x if x == Opcode::OP_PUSHBYTES_65 as u8 => 67,
			_ => return false,
		};

		self.data.len() == len && self.data[len - 1] == Opcode::OP_CHECKSIG as u8
	}

	/// Extra-fast test for pay-to-script-hash (P2SH) scripts.
	pub fn is_pay_to_script_hash(&self) -> bool {
		self.data.len() == 23 &&
			self.data[0] == Opcode::OP_HASH160 as u8 &&
			self.data[1] == Opcode::OP_PUSHBYTES_20 as u8 &&
			self.data[22] == Opcode::OP_EQUAL as u8
	}

	/// Extra-fast test for pay-to-witness-script-hash scripts.
	pub fn is_pay_to_witness_script_hash(&self) -> bool {
		self.data.len() == 34 &&
			self.data[0] == Opcode::OP_0 as u8 &&
			self.data[1] == Opcode::OP_PUSHBYTES_32 as u8
	}

	/// Extra-fast test for multisig scripts.
	pub fn is_multisig_script(&self) -> bool {
		if self.data.len() < 3 {
			return false;
		}

		let siglen = match self.get_opcode(0) {
			Ok(Opcode::OP_0) => 0,
			Ok(o) if o >= Opcode::OP_1 && o <= Opcode::OP_16 => o as u8 - (Opcode::OP_1 as u8 - 1),
			_ => return false,
		};

		let keylen = match self.get_opcode(self.data.len() - 2) {
			Ok(Opcode::OP_0) => 0,
			Ok(o) if o >= Opcode::OP_1 && o <= Opcode::OP_16 => o as u8 - (Opcode::OP_1 as u8 - 1),
			_ => return false,
		};

		if siglen > keylen {
			return false;
		}

		if self.data[self.data.len() - 1] != Opcode::OP_CHECKMULTISIG as u8 {
			return false;
		}

		let mut pc = 1;
		let mut keys = 0;
		while pc < self.len() - 2 {
			let instruction = match self.get_instruction(pc) {
				Ok(i) => i,
				_ => return false,
			};

			match instruction.opcode {
				Opcode::OP_PUSHBYTES_33 |
				Opcode::OP_PUSHBYTES_65 => keys += 1,
				_ => return false,
			}

			pc += instruction.step;
		}

		keys == keylen
	}

	pub fn is_null_data_script(&self) -> bool {
		// TODO: optimise it
		!self.data.is_empty() &&
			self.data[0] == Opcode::OP_RETURN as u8 &&
			self.subscript(1).is_push_only()
	}

	pub fn subscript(&self, from: usize) -> Script {
		self.data[from..].to_vec().into()
	}

	pub fn find_and_delete(&self, data: &[u8]) -> Script {
		let mut result = Vec::new();
		let mut current = 0;
		let len = data.len();
		let end = self.data.len();

		if len > end {
			return self.data.to_vec().into()
		}

		while current < end - len {
			if &self.data[current..current + len] != data {
				result.push(self.data[current]);
				current += 1;
			} else {
				current += len;
			}
		}

		result.extend_from_slice(&self.data[current..]);
		result.into()
	}

	pub fn get_opcode(&self, position: usize) -> Result<Opcode, Error> {
		Opcode::from_u8(self.data[position]).ok_or(Error::BadOpcode)
	}

	pub fn get_instruction(&self, position: usize) -> Result<Instruction, Error> {
		let opcode = try!(self.get_opcode(position));
		let instruction = match opcode {
			Opcode::OP_PUSHDATA1 |
			Opcode::OP_PUSHDATA2 |
			Opcode::OP_PUSHDATA4 => {
				let len = match opcode {
					Opcode::OP_PUSHDATA1 => 1,
					Opcode::OP_PUSHDATA2 => 2,
					_ => 4,
				};

				let slice = try!(self.take(position + 1, len));
				let n = try!(read_usize(slice, len));
				let bytes = try!(self.take_checked(position + 1 + len, n));
				Instruction {
					opcode: opcode,
					step: len + n + 1,
					data: Some(bytes),
				}
			},
			o if o >= Opcode::OP_0 && o <= Opcode::OP_PUSHBYTES_75 => {
				let bytes = try!(self.take_checked(position+ 1, opcode as usize));
				Instruction {
					opcode: o,
					step: opcode as usize + 1,
					data: Some(bytes),
				}
			},
			_ => Instruction {
				opcode: opcode,
				step: 1,
				data: None,
			}
		};

		Ok(instruction)
	}

	#[inline]
	pub fn take(&self, offset: usize, len: usize) -> Result<&[u8], Error> {
		if offset + len > self.data.len() {
			Err(Error::BadOpcode)
		} else {
			Ok(&self.data[offset..offset + len])
		}
	}

	#[inline]
	pub fn take_checked(&self, offset: usize, len: usize) -> Result<&[u8], Error> {
		if len > MAX_SCRIPT_ELEMENT_SIZE {
			Err(Error::ScriptSize)
		} else {
			self.take(offset, len)
		}
	}

	/// Returns Script without OP_CODESEPARATOR opcodes
	pub fn without_separators(&self) -> Script {
		let mut pc = 0;
		let mut result = Vec::new();

		while pc < self.len() {
			match self.get_instruction(pc) {
				Ok(instruction) => {
					if instruction.opcode != Opcode::OP_CODESEPARATOR {
						result.extend_from_slice(&self[pc..pc + instruction.step]);
					}

					pc += instruction.step;
				},
				_ => {
					result.push(self[pc]);
					pc += 1;
				}
			}
		}

		result.into()
	}

	/// Returns true if script contains only push opcodes
	pub fn is_push_only(&self) -> bool {
		let mut pc = 0;
		while pc < self.len() {
			let instruction = match self.get_instruction(pc) {
				Ok(i) => i,
				_ => return false,
			};

			if instruction.opcode > Opcode::OP_16 {
				return false;
			}

			pc += instruction.step;
		}
		true
	}

	pub fn script_type(&self) -> ScriptType {
		if self.is_pay_to_public_key() {
			ScriptType::PubKey
		} else if self.is_pay_to_public_key_hash() {
			ScriptType::PubKeyHash
		} else if self.is_pay_to_script_hash() {
			ScriptType::ScriptHash
		} else if self.is_multisig_script() {
			ScriptType::Multisig
		} else if self.is_null_data_script() {
			ScriptType::NullData
		} else {
			ScriptType::NonStandard
		}
	}

	pub fn iter(&self) -> Instructions {
		Instructions { position: 0, script: self }
	}

	pub fn opcodes(&self) -> Opcodes {
		Opcodes { position: 0, script: self }
	}

	pub fn sigop_count(&self, accurate: bool) -> Result<usize, Error> {
		let mut last_opcode = Opcode::OP_0;
		let mut result = 0;
		for opcode in self.opcodes() {
			let opcode = try!(opcode);

			match opcode {
				Opcode::OP_CHECKSIG | Opcode::OP_CHECKSIGVERIFY => { result += 1; },
				Opcode::OP_CHECKMULTISIG | Opcode::OP_CHECKMULTISIGVERIFY => {
					if accurate {
						match last_opcode {
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
								result += (last_opcode as u8 - (Opcode::OP_1 as u8 - 1)) as usize;
							},
							_ => {
								result += MAX_PUBKEYS_PER_MULTISIG;
							}
						}
					}
					else {
						result += MAX_PUBKEYS_PER_MULTISIG;
					}
				},
				_ => { }
			};

			last_opcode = opcode;
		}

		Ok(result)
	}

	pub fn sigop_count_p2sh(&self, input_ref: &Script) -> Result<usize, Error> {
		if !self.is_pay_to_script_hash() { return self.sigop_count(true); }

		let mut script_data: Option<&[u8]> = None;
		// we need last command
		for next in input_ref.iter() {
			let instruction = match next {
				Err(_) => return Ok(0),
				Ok(i) => i,
			};

			if instruction.opcode as u8 > Opcode::OP_16 as u8 {
				return Ok(0);
			}

			script_data = instruction.data;
		}

		match script_data {
			Some(slc) => {
				let nested_script: Script = slc.to_vec().into();
				nested_script.sigop_count(true)
			},
			None => Ok(0),
		}
	}
}

pub struct Instructions<'a> {
	position: usize,
	script: &'a Script,
}

pub struct Opcodes<'a> {
	position: usize,
	script: &'a Script,
}

impl<'a> Iterator for Instructions<'a> {
	type Item = Result<Instruction<'a>, Error>;

	fn next(&mut self) -> Option<Result<Instruction<'a>, Error>> {

		if self.script.len() <= self.position { return None; }

		let instruction = match self.script.get_instruction(self.position) {
			Ok(x) => x,
			Err(e) => return Some(Err(e)),
		};

		self.position += instruction.step;

		Some(Ok(instruction))
	}
}


impl<'a> Iterator for Opcodes<'a> {
	type Item = Result<Opcode, Error>;

	fn next(&mut self) -> Option<Result<Opcode, Error>> {

		if self.script.len() <= self.position { return None; }

		let instruction = match self.script.get_instruction(self.position) {
			Ok(x) => x,
			Err(e) => return Some(Err(e)),
		};

		self.position += instruction.step;

		Some(Ok(instruction.opcode))
	}
}

impl ops::Deref for Script {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.data
	}
}

pub struct Instruction<'a> {
	pub opcode: Opcode,
	pub step: usize,
	pub data: Option<&'a [u8]>,
}

fn read_usize(data: &[u8], size: usize) -> Result<usize, Error> {
	if data.len() < size {
		return Err(Error::BadOpcode);
	}

	let result = data
		.iter()
		.take(size)
		.enumerate()
		.fold(0, |acc, (i, x)| acc + ((*x as usize) << (i * 8)));
	Ok(result)
}

impl fmt::Display for Script {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let mut pc = 0;

		while pc < self.len() {
			let instruction = match self.get_instruction(pc) {
				Ok(i) => i,
				Err(e) => return e.fmt(f),
			};

			match instruction.data {
				Some(data) => try!(writeln!(f, "{:?} 0x{:?}", instruction.opcode, Bytes::from(data.to_vec()))),
				None => try!(writeln!(f, "{:?}", instruction.opcode)),
			}

			pc += instruction.step;
		}

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use {Builder, Opcode};
	use super::{Script, ScriptType};

	#[test]
	fn test_is_pay_to_script_hash() {
		let script: Script = "a9143b80842f4ea32806ce5e723a255ddd6490cfd28d87".into();
		let script2: Script = "a9143b80842f4ea32806ce5e723a255ddd6490cfd28d88".into();
		assert!(script.is_pay_to_script_hash());
		assert!(!script2.is_pay_to_script_hash());
	}

	#[test]
	fn test_is_pay_to_witness_script_hash() {
		let script: Script = "00203b80842f4ea32806ce5e723a255ddd6490cfd28dac38c58bf9254c0577330693".into();
		let script2: Script = "01203b80842f4ea32806ce5e723a255ddd6490cfd28dac38c58bf9254c0577330693".into();
		assert!(script.is_pay_to_witness_script_hash());
		assert!(!script2.is_pay_to_witness_script_hash());
	}

	#[test]
	fn test_script_debug() {
		use std::fmt::Write;

		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let s = "Script { data: 0103010293 }";
		let mut res = String::new();
		write!(&mut res, "{:?}", script).unwrap();
		assert_eq!(s.to_string(), res);
	}

	#[test]
	fn test_script_display() {
		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let s = r#"OP_PUSHBYTES_1 0x03
OP_PUSHBYTES_1 0x02
OP_ADD
"#;
		assert_eq!(script.to_string(), s.to_string());
	}

	#[test]
	fn test_script_without_op_codeseparator() {
		let script: Script = "ab00270025512102e485fdaa062387c0bbb5ab711a093b6635299ec155b7b852fce6b992d5adbfec51ae".into();
		let scr_goal: Script = "00270025512102e485fdaa062387c0bbb5ab711a093b6635299ec155b7b852fce6b992d5adbfec51ae".into();
		assert_eq!(script.without_separators(), scr_goal);
	}

	#[test]
	fn test_script_is_multisig() {
		let script: Script = "524104a882d414e478039cd5b52a92ffb13dd5e6bd4515497439dffd691a0f12af9575fa349b5694ed3155b136f09e63975a1700c9f4d4df849323dac06cf3bd6458cd41046ce31db9bdd543e72fe3039a1f1c047dab87037c36a669ff90e28da1848f640de68c2fe913d363a51154a0c62d7adea1b822d05035077418267b1a1379790187410411ffd36c70776538d079fbae117dc38effafb33304af83ce4894589747aee1ef992f63280567f52f5ba870678b4ab4ff6c8ea600bd217870a8b4f1f09f3a8e8353ae".into();
		let not: Script = "ab00270025512102e485fdaa062387c0bbb5ab711a093b6635299ec155b7b852fce6b992d5adbfec51ae".into();
		assert!(script.is_multisig_script());
		assert!(!not.is_multisig_script());
	}

	// https://github.com/libbtc/libbtc/blob/998badcdac95a226a8f8c00c8f6abbd8a77917c1/test/tx_tests.c#L640
	#[test]
	fn test_script_type() {
		assert_eq!(ScriptType::PubKeyHash, Script::from("76a914aab76ba4877d696590d94ea3e02948b55294815188ac").script_type());
		assert_eq!(ScriptType::Multisig, Script::from("522102004525da5546e7603eefad5ef971e82f7dad2272b34e6b3036ab1fe3d299c22f21037d7f2227e6c646707d1c61ecceb821794124363a2cf2c1d2a6f28cf01e5d6abe52ae").script_type());
		assert_eq!(ScriptType::ScriptHash, Script::from("a9146262b64aec1f4a4c1d21b32e9c2811dd2171fd7587").script_type());
		assert_eq!(ScriptType::PubKey, Script::from("4104ae1a62fe09c5f51b13905f07f06b99a2f7159b2225f374cd378d71302fa28414e7aab37397f554a7df5f142c21c1b7303b8a0626f1baded5c72a704f7e6cd84cac").script_type());
	}

	#[test]
	fn test_sigops_count() {
		assert_eq!(1usize, Script::from("76a914aab76ba4877d696590d94ea3e02948b55294815188ac").sigop_count(false).unwrap());
		assert_eq!(2usize, Script::from("522102004525da5546e7603eefad5ef971e82f7dad2272b34e6b3036ab1fe3d299c22f21037d7f2227e6c646707d1c61ecceb821794124363a2cf2c1d2a6f28cf01e5d6abe52ae").sigop_count(true).unwrap());
		assert_eq!(20usize, Script::from("522102004525da5546e7603eefad5ef971e82f7dad2272b34e6b3036ab1fe3d299c22f21037d7f2227e6c646707d1c61ecceb821794124363a2cf2c1d2a6f28cf01e5d6abe52ae").sigop_count(false).unwrap());
		assert_eq!(0usize, Script::from("a9146262b64aec1f4a4c1d21b32e9c2811dd2171fd7587").sigop_count(false).unwrap());
		assert_eq!(1usize, Script::from("4104ae1a62fe09c5f51b13905f07f06b99a2f7159b2225f374cd378d71302fa28414e7aab37397f554a7df5f142c21c1b7303b8a0626f1baded5c72a704f7e6cd84cac").sigop_count(false).unwrap());
	}
}
