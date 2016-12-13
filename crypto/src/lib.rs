extern crate crypto as rcrypto;
extern crate primitives;
extern crate siphasher;

pub use rcrypto::digest::Digest;
use std::hash::Hasher;
use rcrypto::sha1::Sha1;
use rcrypto::sha2::Sha256;
use rcrypto::ripemd160::Ripemd160;
use siphasher::sip::SipHasher24;
use primitives::hash::{H32, H160, H256};

pub struct DHash160 {
	sha256: Sha256,
	ripemd: Ripemd160,
}

impl Default for DHash160 {
	fn default() -> Self {
		DHash160 {
			sha256: Sha256::new(),
			ripemd: Ripemd160::new(),
		}
	}
}

impl DHash160 {
	pub fn new() -> Self {
		DHash160::default()
	}
}

impl Digest for DHash160 {
	fn input(&mut self, d: &[u8]) {
		self.sha256.input(d)
	}

	fn result(&mut self, out: &mut [u8]) {
		let mut tmp = [0u8; 32];
		self.sha256.result(&mut tmp);
		self.ripemd.input(&tmp);
		self.ripemd.result(out);
		self.ripemd.reset();
	}

	fn reset(&mut self) {
		self.sha256.reset();
	}

	fn output_bits(&self) -> usize {
		160
	}

	fn block_size(&self) -> usize {
		64
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

	pub fn finish(mut self) -> H256 {
		let mut result = H256::default();
		self.result(&mut *result);
		result
	}
}

impl Digest for DHash256 {
	fn input(&mut self, d: &[u8]) {
		self.hasher.input(d)
	}

	fn result(&mut self, out: &mut [u8]) {
		self.hasher.result(out);
		self.hasher.reset();
		self.hasher.input(out);
		self.hasher.result(out);
	}

	fn reset(&mut self) {
		self.hasher.reset();
	}

	fn output_bits(&self) -> usize {
		256
	}

	fn block_size(&self) -> usize {
		64
	}
}

/// RIPEMD160
#[inline]
pub fn ripemd160(input: &[u8]) -> H160 {
	let mut result = H160::default();
	let mut hasher = Ripemd160::new();
	hasher.input(input);
	hasher.result(&mut *result);
	result
}

/// SHA-1
#[inline]
pub fn sha1(input: &[u8]) -> H160 {
	let mut result = H160::default();
	let mut hasher = Sha1::new();
	hasher.input(input);
	hasher.result(&mut *result);
	result
}

/// SHA-256
#[inline]
pub fn sha256(input: &[u8]) -> H256 {
	let mut result = H256::default();
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut *result);
	result
}

/// SHA-256 and RIPEMD160
#[inline]
pub fn dhash160(input: &[u8]) -> H160 {
	let mut result = H160::default();
	let mut hasher = DHash160::new();
	hasher.input(input);
	hasher.result(&mut *result);
	result
}

/// Double SHA-256
#[inline]
pub fn dhash256(input: &[u8]) -> H256 {
	let mut result = H256::default();
	let mut hasher = DHash256::new();
	hasher.input(input);
	hasher.result(&mut *result);
	result
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
