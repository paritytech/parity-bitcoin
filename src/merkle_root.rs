use crypto::dhash;
use hash::{H256, H512};

fn concat(a: &H256, b: &H256) -> H512 {
	let mut result = [0u8; 64];
	result[0..32].copy_from_slice(a);
	result[32..64].copy_from_slice(b);
	result
}

pub fn merkle_root(hashes: &[H256]) -> H256 {
	if hashes.len() == 1 {
		return hashes[0].clone();
	}

	let mut row = vec![];
	let mut i = 0;
	while i + 1 < hashes.len() {
		row.push(dhash(&concat(&hashes[i], &hashes[i + 1])));
		i += 2
	}

	// duplicate the last element if len is not even
	if hashes.len() % 2 == 1 {
		let last = hashes[hashes.len() - 1];
		row.push(dhash(&concat(&last, &last)));
	}

	merkle_root(&row)
}

#[cfg(test)]
mod tests {
	use rustc_serialize::hex::FromHex;
	use super::merkle_root;

	// block 80_000
	// https://blockchain.info/block/000000000043a8c0fd1d6f726790caa2a406010d19efd2780db27bdbbd93baf6
	#[test]
	fn test_merkle_root_with_2_hashes() {
		let mut tx1 = "c06fbab289f723c6261d3030ddb6be121f7d2508d77862bb1e484f5cd7f92b25".from_hex().unwrap();
		let mut tx2 = "5a4ebf66822b0b2d56bd9dc64ece0bc38ee7844a23ff1d7320a88c5fdb2ad3e2".from_hex().unwrap();
		let mut expected = "8fb300e3fdb6f30a4c67233b997f99fdd518b968b9a3fd65857bfe78b2600719".from_hex().unwrap();
		tx1.reverse();
		tx2.reverse();
		expected.reverse();

		let mut a = [0u8; 32];
		a.copy_from_slice(&tx1);
		let mut b = [0u8; 32];
		b.copy_from_slice(&tx2);

		let result = merkle_root(&[a, b]);
		assert_eq!(result.to_vec(), expected);
	}
}
