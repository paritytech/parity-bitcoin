use rcrypto::sha2::Sha256;
use rcrypto::digest::Digest;
use hash::H256;

/// SHA-256
#[inline]
pub fn hash(input: &[u8]) -> H256 {
	let mut result = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut result);
	result
}

pub struct DHash256 {
	hasher: Sha256,
}

impl DHash256 {
	pub fn new() -> Self {
		DHash256 {
			hasher: Sha256::new(),
		}
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

/// Double SHA-256
#[inline]
pub fn dhash(input: &[u8]) -> H256 {
	let mut result = [0u8; 32];
	let mut hasher = DHash256::new();
	hasher.input(input);
	hasher.result(&mut result);
	result
}

#[cfg(test)]
mod tests {
	use super::dhash;
	use hex::FromHex;

    #[test]
    fn test_double_hash() {
		let expected = "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".from_hex().unwrap();
		let result = dhash(b"hello");
		assert_eq!(result.to_vec(), expected);
    }
}
