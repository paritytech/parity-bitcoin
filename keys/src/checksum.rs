use crypto::dhash256;

/// Data checksum
pub fn checksum(data: &[u8]) -> [u8; 4] {
	let mut result = [0u8; 4];
	result.copy_from_slice(&dhash256(data)[0..4]);
	result
}
