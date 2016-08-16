use rcrypto::sha2::Sha256;
use rcrypto::digest::Digest;

/// SHA-256
pub fn hash(input: &[u8]) -> [u8; 32] {
	let mut result = [0u8; 32];
	let mut hasher = Sha256::new();
	hasher.input(input);
	hasher.result(&mut result);
	result
}

/// Double SHA-256
pub fn dhash(input: &[u8]) -> [u8; 32] {
	hash(&hash(input))
}

#[cfg(test)]
mod tests {
	use super::dhash;
	use rustc_serialize::hex::FromHex;

	// block 80_000
	// https://blockchain.info/block/000000000043a8c0fd1d6f726790caa2a406010d19efd2780db27bdbbd93baf6
    #[test]
    fn test_double_hash() {
		let mut tx1 = "c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25".from_hex().unwrap();
		let mut tx2 = "5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2".from_hex().unwrap();
		let mut merkle_root = "8fb300e3fdb6f30a4c67233b997f99fdd518b968b9a3fd65857bfe78b2600719".from_hex().unwrap();
		tx1.reverse();
		tx2.reverse();
		merkle_root.reverse();

		let mut concat = [0u8; 64];
		concat[0..32].copy_from_slice(&tx1);
		concat[32..64].copy_from_slice(&tx2);

		let result = dhash(&concat);
		assert_eq!(result.to_vec(), merkle_root);
    }
}
