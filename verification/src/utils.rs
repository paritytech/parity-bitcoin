//! Verification utilities
use primitives::hash::H256;
use byteorder::{BigEndian, ByteOrder};

pub fn check_nbits(hash: &H256, n_bits: u32) -> bool {
	let hash_bytes: &[u8] = &**hash;

	let mut nb = [0u8; 4];
	BigEndian::write_u32(&mut nb, n_bits);
	let shift = (nb[0] - 3) as usize; // total shift for mantissa

	if shift >= 30 { return false; } // invalid shift

	let should_be_zero = shift + 3..32;
	let should_be_le = shift..shift + 3;

	for z_check in should_be_zero {
		if hash_bytes[z_check as usize] != 0 { return false; }
	}

	// making u32 from 3 bytes
	let mut order = 0;
	let hash_val: u32 = hash_bytes[should_be_le].iter().fold(0u32, |s, a| { let r = s + ((*a as u32) << order); order = order + 8; r });

	// using 3 bytes leftover of nbits
	nb[0] = 0;
	let threshold = BigEndian::read_u32(&nb);
	if hash_val < threshold {
		return true;
	}
	else if hash_val > threshold {
		return false;
	}

	// the case when hash effective bits are equal to nbits
	// then the rest of the hash must be zero
	for byte in hash_bytes[0..shift].iter() { if *byte != 0 { return false; } }

	return true;
}

pub fn age(protocol_time: u32) -> i64 {
	::time::get_time().sec - protocol_time as i64
}

#[cfg(test)]
mod tests {

	use super::check_nbits;
	use primitives::hash::H256;

	#[test]
	fn nbits() {
		// strictly equal
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000000");
		let nbits = 0x181bc330u32;
		assert!(check_nbits(&hash, nbits));

		// nbits match but not equal (greater)
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000001");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(&hash, nbits));

		// greater
		let hash = H256::from_reversed_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(&hash, nbits));


		// some real examples
		let hash = H256::from_reversed_str("000000000000000001f942eb4bfa0aeccb6a14c268f4c72d5fff17270da771b9");
		let nbits = 404129525;
		assert!(check_nbits(&hash, nbits));

		let hash = H256::from_reversed_str("00000000000000000e753ef636075711efd2cbf5a8473c7c5b67755a3701e0c2");
		let nbits = 404129525;
		assert!(check_nbits(&hash, nbits));
	}
}
