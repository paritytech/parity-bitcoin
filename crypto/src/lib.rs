extern crate digest;
extern crate sha1;
extern crate sha2;
extern crate ripemd160;
extern crate primitives;
extern crate siphasher;

use std::hash::Hasher;
pub use digest::Digest;
use digest::generic_array::GenericArray;
use digest::generic_array::typenum::{U20, U32, U64};
use sha1::Sha1;
use sha2::Sha256;
use ripemd160::Ripemd160;
use siphasher::sip::SipHasher24;
use primitives::hash::{H32, H160, H256};


pub struct DHash160 {
	sha256: Sha256,
	ripemd: Ripemd160,
}

impl Default for DHash160 {
	fn default() -> Self {
		DHash160 {
			sha256: Sha256::default(),
			ripemd: Ripemd160::default(),
		}
	}
}

impl DHash160 {
	pub fn new() -> Self {
		DHash160::default()
	}
}

impl digest::BlockInput for DHash160 {
	type BlockSize = U64;
}

impl digest::Input for DHash160 {
	fn process(&mut self, input: &[u8]) {
		self.sha256.process(input)
	}
}

impl digest::FixedOutput for DHash160 {
	type OutputSize = U20;

	fn fixed_result(mut self) -> GenericArray<u8, Self::OutputSize> {
		use digest::Input;

		let out = self.sha256.fixed_result();

		self.ripemd.process(out.as_slice());
		self.ripemd.fixed_result()
	}
}

pub struct DHash256 {
	hasher: Sha256,
}

impl Default for DHash256 {
	fn default() -> Self {
		DHash256 {
			hasher: Sha256::new(),
		}
	}
}

impl DHash256 {
	pub fn new() -> Self {
		DHash256::default()
	}

	pub fn finish(self) -> H256 {
		H256::from(self.result().as_slice())
	}
}

impl digest::BlockInput for DHash256 {
	type BlockSize = U64;
}

impl digest::Input for DHash256 {
	fn process(&mut self, d: &[u8]) {
		self.hasher.process(d)
	}
}

impl digest::FixedOutput for DHash256 {
	type OutputSize = U32;

	fn fixed_result(self) -> GenericArray<u8, Self::OutputSize> {
		use digest::Input;

		let out = self.hasher.fixed_result();

		let mut hasher = Sha256::new();
		hasher.process(out.as_slice());
		hasher.fixed_result()
	}
}

/// RIPEMD160
#[inline]
pub fn ripemd160(input: &[u8]) -> H160 {
	let mut hasher = Ripemd160::new();
	hasher.input(input);
	H160::from(hasher.result().as_slice())
}

/// SHA-1
#[inline]
pub fn sha1(input: &[u8]) -> H160 {
	let mut hasher = Sha1::default();
	hasher.input(input);
	H160::from(hasher.result().as_slice())
}

/// SHA-256
#[inline]
pub fn sha256(input: &[u8]) -> H256 {
	let mut hasher = Sha256::default();
	hasher.input(input);
	H256::from(hasher.result().as_slice())
}

/// SHA-256 and RIPEMD160
#[inline]
pub fn dhash160(input: &[u8]) -> H160 {
	let mut hasher = DHash160::new();
	hasher.input(input);
	H160::from(hasher.result().as_slice())
}

/// Double SHA-256
#[inline]
pub fn dhash256(input: &[u8]) -> H256 {
	let mut hasher = DHash256::new();
	hasher.input(input);
	H256::from(hasher.result().as_slice())
}

/// SipHash-2-4
#[inline]
pub fn siphash24(key0: u64, key1: u64, input: &[u8]) -> u64 {
	let mut hasher = SipHasher24::new_with_keys(key0, key1);
	hasher.write(input);
	hasher.finish()
}

/// Data checksum
#[inline]
pub fn checksum(data: &[u8]) -> H32 {
	let mut result = H32::default();
	result.copy_from_slice(&dhash256(data)[0..4]);
	result
}

#[cfg(test)]
mod tests {
	use primitives::bytes::Bytes;
	use super::{ripemd160, sha1, sha256, dhash160, dhash256, siphash24, checksum};

	#[test]
	fn test_ripemd160() {
		let expected = "108f07b8382412612c048d07d13f814118445acd".into();
		let result = ripemd160(b"hello");
		assert_eq!(result, expected);
	}

	#[test]
	fn test_sha1() {
		let expected = "aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d".into();
		let result = sha1(b"hello");
		assert_eq!(result, expected);
	}

	#[test]
	fn test_sha256() {
		let expected = "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824".into();
		let result = sha256(b"hello");
		assert_eq!(result, expected);
	}

	#[test]
	fn test_dhash160() {
		let expected = "b6a9c8c230722b7c748331a8b450f05566dc7d0f".into();
		let result = dhash160(b"hello");
		assert_eq!(result, expected);

		let expected = "865c71bfc7e314709207ab9e7e205c6f8e453d08".into();
		let bytes: Bytes = "210292be03ed9475445cc24a34a115c641a67e4ff234ccb08cb4c5cea45caa526cb26ead6ead6ead6ead6eadac".into();
		let result = dhash160(&bytes);
		assert_eq!(result, expected);
	}

	#[test]
	fn test_dhash256() {
		let expected = "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".into();
		let result = dhash256(b"hello");
		assert_eq!(result, expected);
	}

	#[test]
	fn test_siphash24() {
		let expected = 0x74f839c593dc67fd_u64;
		let result = siphash24(0x0706050403020100_u64, 0x0F0E0D0C0B0A0908_u64, &[0; 1]);
		assert_eq!(result, expected);
	}

	#[test]
	fn test_checksum() {
		assert_eq!(checksum(b"hello"), "9595c9df".into());
	}
}
