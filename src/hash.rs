use hex::{ToHex, FromHex};

pub type H160 = [u8; 20];
pub type H256 = [u8; 32];
pub type H264 = [u8; 33];
pub type H512 = [u8; 64];
pub type H520 = [u8; 65];

/// Reverses the hash. Commonly used to display
#[inline]
pub fn reverse(hash: &H256) -> H256 {
	let mut result = hash.clone();
	result.reverse();
	result
}

/// Loads hash from display str
#[inline]
pub fn h256_from_str(s: &'static str) -> H256 {
	let mut result = [0u8; 32];
	result.copy_from_slice(&s.from_hex().unwrap());
	result.reverse();
	result
}

/// Transforms hash to display string.
#[inline]
pub fn h256_to_str(hash: &H256) -> String {
	reverse(hash).to_hex()
}

#[inline]
pub fn h256_from_u8(u: u8) -> H256 {
	let mut result = [0u8; 32];
	result[31] = u;
	result
}
