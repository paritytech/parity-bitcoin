//! Serialized script, used inside transaction inputs and outputs.

use std::fmt;
use hex::ToHex;
use script::{Opcode, Error, Num};

/// Maximum number of bytes pushable to the stack
const MAX_SCRIPT_ELEMENT_SIZE: usize = 520;

/// Maximum number of non-push operations per script
const _MAX_OPS_PER_SCRIPT: u32 = 201;

/// Maximum number of public keys per multisig
const _MAX_PUBKEYS_PER_MULTISIG: u32 = 20;

/// Maximum script length in bytes
pub const MAX_SCRIPT_SIZE: usize = 10000;

/// Threshold for nLockTime: below this value it is interpreted as block number,
/// otherwise as UNIX timestamp.
const _LOCKTIME_THRESHOLD: u32 = 500000000; // Tue Nov  5 00:53:20 1985 UTC

/// Serialized script, used inside transaction inputs and outputs.
pub struct Script {
	data: Vec<u8>,
}

impl Script {
	/// Script constructor.
	pub fn new(data: Vec<u8>) -> Self {
		Script {
			data: data,
		}
	}

	/// Extra-fast test for pay-to-script-hash scripts.
	pub fn is_pay_to_script_hash(&self) -> bool {
		self.data.len() == 23 &&
			self.data[0] == Opcode::OP_HASH160 as u8 &&
			self.data[1] == 0x14 &&
			self.data[22] == Opcode::OP_EQUAL as u8
	}

	/// Extra-fast test for pay-to-witness-script-hash scripts.
	pub fn is_pay_to_witness_script_hash(&self) -> bool {
		self.data.len() == 34 &&
			self.data[0] == Opcode::OP_0 as u8 &&
			self.data[1] == 0x20
	}

	pub fn is_empty(&self) -> bool {
		self.data.is_empty()
	}

	pub fn len(&self) -> usize {
		self.data.len()
	}

	pub fn subscript(&self, from: usize) -> Script {
		Script::new(self.data[from..].to_vec())
	}

	pub fn find_and_delete(&self, data: &[u8]) -> Script {
		let mut result = Vec::new();
		let mut current = 0;
		let len = data.len();
		let end = self.data.len();

		if len > end {
			return Script::new(data.to_vec());
		}

		while current < end - len {
			if &self.data[current..len] != data {
				result.push(self.data[current]);
				current += 1;
			} else {
				current += len;
			}
		}

		result.extend_from_slice(&self.data[current..]);
		Script::new(result)
	}

	pub fn get_opcode(&self, position: usize) -> Result<Opcode, Error> {
		Opcode::from_u8(self.data[position]).ok_or(Error::BadOpcode)
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
}

pub fn read_usize(data: &[u8], size: usize) -> Result<usize, Error> {
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

macro_rules! try_or_fmt {
	($fmt: expr, $expr: expr) => {
		match $expr {
			Ok(o) => o,
			Err(e) => return e.fmt($fmt),
		}
	}
}

impl fmt::Debug for Script {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.data.to_hex())
	}
}

impl fmt::Display for Script {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		let mut pc = 0;
		while pc < self.len() {
			let opcode = try_or_fmt!(f, self.get_opcode(pc));

			match opcode {
				Opcode::OP_PUSHDATA1 |
				Opcode::OP_PUSHDATA2 |
				Opcode::OP_PUSHDATA4 => {
					let len = match opcode {
						Opcode::OP_PUSHDATA1 => 1,
						Opcode::OP_PUSHDATA2 => 2,
						_ => 4,
					};

					let slice = try_or_fmt!(f, self.take(pc + 1, len));
					let n = try_or_fmt!(f, read_usize(slice, len));
					let bytes = try_or_fmt!(f, self.take_checked(pc + 1 + len, n));
					pc += len + n;
					try!(writeln!(f, "{:?} 0x{}", opcode, bytes.to_hex()));
				},
				o if o >= Opcode::OP_0 && o <= Opcode::OP_PUSHBYTES_75 => {
					let bytes = try_or_fmt!(f, self.take_checked(pc + 1, opcode as usize));
					pc += opcode as usize;
					try!(writeln!(f, "{:?} 0x{}", opcode, bytes.to_hex()));
				},
				_ => {
					try!(writeln!(f, "{:?}", opcode));
				},
			}
			pc += 1;
		}
		Ok(())
	}
}

pub struct ScriptWitness {
	script: Vec<Vec<u8>>,
}

#[cfg(test)]
mod tests {
	use hex::FromHex;
	use script::{Builder, Opcode};
	use super::Script;

	#[test]
	fn test_is_pay_to_script_hash() {
		let data = "a9143b80842f4ea32806ce5e723a255ddd6490cfd28d87".from_hex().unwrap();
		let data2 = "a9143b80842f4ea32806ce5e723a255ddd6490cfd28d88".from_hex().unwrap();
		assert!(Script::new(data).is_pay_to_script_hash());
		assert!(!Script::new(data2).is_pay_to_script_hash());
	}

	#[test]
	fn test_is_pay_to_witness_script_hash() {
		let data = "00203b80842f4ea32806ce5e723a255ddd6490cfd28dac38c58bf9254c0577330693".from_hex().unwrap();
		let data2 = "01203b80842f4ea32806ce5e723a255ddd6490cfd28dac38c58bf9254c0577330693".from_hex().unwrap();
		assert!(Script::new(data).is_pay_to_witness_script_hash());
		assert!(!Script::new(data2).is_pay_to_witness_script_hash());
	}

	#[test]
	fn test_script_debug() {
		use std::fmt::Write;

		let script = Builder::default()
			.push_num(3.into())
			.push_num(2.into())
			.push_opcode(Opcode::OP_ADD)
			.into_script();
		let s = "0103010293";
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
}
