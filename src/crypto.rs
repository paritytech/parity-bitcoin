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

/// Double SHA-256
#[inline]
pub fn dhash(input: &[u8]) -> H256 {
	hash(&hash(input))
}

#[cfg(test)]
mod tests {
	use super::dhash;
	use rustc_serialize::hex::FromHex;

    #[test]
    fn test_double_hash() {
		let expected = "9595c9df90075148eb06860365df33584b75bff782a510c6cd4883a419833d50".from_hex().unwrap();
		let result = dhash(b"hello");
		assert_eq!(result.to_vec(), expected);
    }
}
