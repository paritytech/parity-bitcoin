use std::{ops, str, fmt};
use hex::{ToHex, FromHex, FromHexError};
use compact_integer::CompactInteger;
use stream::{Stream, Serializable};
use reader::{Reader, Deserializable, Error as ReaderError};

#[derive(Default, PartialEq, Clone)]
pub struct Bytes(Vec<u8>);

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

impl Serializable for Bytes {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&CompactInteger::from(self.0.len()))
			.append_slice(&self.0);
	}
}

impl Deserializable for Bytes {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let len = try!(reader.read::<CompactInteger>());
		reader.read_slice(len.into()).map(|b| Bytes(b.to_vec()))
	}
}

#[cfg(test)]
mod tests {
	use stream::serialize;
	use reader::deserialize;
	use super::Bytes;

	#[test]
	fn test_bytes_from_hex() {
		let bytes: Bytes = "0145".into();
		assert_eq!(bytes, vec![0x01, 0x45].into());
	}

	#[test]
	fn test_bytes_serialize() {
		let expected = vec![0x02, 0x01, 0x45];
		let bytes: Bytes = "0145".into();
		assert_eq!(expected, serialize(&bytes));
	}

	#[test]
	fn test_bytes_deserialize() {
		let raw = vec![0x02, 0x01, 0x45];
		let expected: Bytes = "0145".into();
		assert_eq!(expected, deserialize(&raw).unwrap());
	}

	#[test]
	fn test_bytes_debug_formatter() {
		let bytes: Bytes = "0145".into();
		assert_eq!(format!("{:?}", bytes), "0145".to_owned());
	}
}
