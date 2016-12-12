///! Serializable wrapper around vector of bytes
use rustc_serialize::hex::{ToHex, FromHex};
use serde::{Serialize, Serializer, Deserialize, Deserializer, Error};
use serde::de::Visitor;
use primitives::bytes::Bytes as GlobalBytes;

/// Wrapper structure around vector of bytes.
#[derive(Debug, PartialEq, Eq, Default, Hash, Clone)]
pub struct Bytes(pub Vec<u8>);

impl Bytes {
	/// Simple constructor.
	pub fn new(bytes: Vec<u8>) -> Bytes {
		Bytes(bytes)
	}

	/// Convert back to vector
	pub fn to_vec(self) -> Vec<u8> {
		self.0
	}
}

impl From<GlobalBytes> for Bytes {
	fn from(v: GlobalBytes) -> Self {
		Bytes(v.take())
	}
}

impl From<Vec<u8>> for Bytes {
	fn from(bytes: Vec<u8>) -> Bytes {
		Bytes(bytes)
	}
}

impl Into<Vec<u8>> for Bytes {
	fn into(self) -> Vec<u8> {
		self.0
	}
}

impl Serialize for Bytes {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error>
	where S: Serializer {
		let mut serialized = String::new();
		serialized.push_str(self.0.to_hex().as_ref());
		serializer.serialize_str(serialized.as_ref())
	}
}

impl Deserialize for Bytes {
	fn deserialize<D>(deserializer: &mut D) -> Result<Bytes, D::Error>
	where D: Deserializer {
		deserializer.deserialize(BytesVisitor)
	}
}

struct BytesVisitor;

impl Visitor for BytesVisitor {
	type Value = Bytes;

	fn visit_str<E>(&mut self, value: &str) -> Result<Self::Value, E> where E: Error {
		if value.len() > 0 && value.len() & 1 == 0 {
			Ok(Bytes::new(try!(FromHex::from_hex(&value).map_err(|_| Error::custom("invalid hex")))))
		} else {
			Err(Error::custom("invalid format"))
		}
	}

	fn visit_string<E>(&mut self, value: String) -> Result<Self::Value, E> where E: Error {
		self.visit_str(value.as_ref())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json;
	use rustc_serialize::hex::FromHex;

	#[test]
	fn test_bytes_serialize() {
		let bytes = Bytes("0123456789abcdef".from_hex().unwrap());
		let serialized = serde_json::to_string(&bytes).unwrap();
		assert_eq!(serialized, r#""0123456789abcdef""#);
	}

	#[test]
	fn test_bytes_deserialize() {
		let bytes1: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""""#);
		let bytes2: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""123""#);
		let bytes3: Result<Bytes, serde_json::Error> = serde_json::from_str(r#""gg""#);

		let bytes4: Bytes = serde_json::from_str(r#""12""#).unwrap();
		let bytes5: Bytes = serde_json::from_str(r#""0123""#).unwrap();

		assert!(bytes1.is_err());
		assert!(bytes2.is_err());
		assert!(bytes3.is_err());
		assert_eq!(bytes4, Bytes(vec![0x12]));
		assert_eq!(bytes5, Bytes(vec![0x1, 0x23]));
	}
}
