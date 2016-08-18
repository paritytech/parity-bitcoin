use crypto::dhash;

/// Data checksum
pub fn checksum(data: &[u8]) -> [u8; 4] {
	let mut result = [0u8; 4];
	result.copy_from_slice(&dhash(data)[0..4]);
	result
}
