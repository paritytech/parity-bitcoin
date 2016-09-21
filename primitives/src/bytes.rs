use std::{ops, str, fmt};
use hex::{ToHex, FromHex, FromHexError};

#[derive(Default, PartialEq, Clone)]
pub struct Bytes(Vec<u8>);

impl Bytes {
	pub fn new_with_len(len: usize) -> Self {
		Bytes(vec![0; len])
	}
}

impl From<Vec<u8>> for Bytes {
	fn from(v: Vec<u8>) -> Self {
		Bytes(v)
	}
}

impl From<Bytes> for Vec<u8> {
	fn from(bytes: Bytes) -> Self {
		bytes.0
	}
}

impl From<&'static str> for Bytes {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl str::FromStr for Bytes {
	type Err = FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.from_hex().map(Bytes)
	}
}

impl fmt::Debug for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex())
	}
}

impl ops::Deref for Bytes {
	type Target = Vec<u8>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl ops::DerefMut for Bytes {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl AsRef<[u8]> for Bytes {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl AsMut<[u8]> for Bytes {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0
	}
}

#[cfg(test)]
mod tests {
	use super::Bytes;

	#[test]
	fn test_bytes_from_hex() {
		let bytes: Bytes = "0145".into();
		assert_eq!(bytes, vec![0x01, 0x45].into());
	}

	#[test]
	fn test_bytes_debug_formatter() {
		let bytes: Bytes = "0145".into();
		assert_eq!(format!("{:?}", bytes), "0145".to_owned());
	}
}
