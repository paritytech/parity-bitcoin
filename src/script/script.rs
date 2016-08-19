//! Serialized script, used inside transaction inputs and outputs.

use script::Opcode;

/// Maximum number of bytes pushable to the stack
const MAX_SCRIPT_ELEMENT_SIZE: u32 = 520;

/// Maximum number of non-push operations per script
const MAX_OPS_PER_SCRIPT: u32 = 201;

/// Maximum number of public keys per multisig
const MAX_PUBKEYS_PER_MULTISIG: u32 = 20;

/// Maximum script length in bytes
const MAX_SCRIPT_SIZE: u32 = 10000;

/// Threshold for nLockTime: below this value it is interpreted as block number,
/// otherwise as UNIX timestamp.
const LOCKTIME_THRESHOLD: u32 = 500000000; // Tue Nov  5 00:53:20 1985 UTC

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
}

#[cfg(test)]
mod tests {
	use hex::FromHex;
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
}
