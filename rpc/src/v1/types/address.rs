use std::fmt;
use serde::{Serialize, Serializer, Deserializer};
use serde::de::{Visitor, Unexpected};
use keys::Address;

pub fn serialize<S>(address: &Address, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
	address.to_string().serialize(serializer)
}

pub fn deserialize<'a, D>(deserializer: D) -> Result<Address, D::Error> where D: Deserializer<'a> {
	deserializer.deserialize_any(AddressVisitor)
}

#[derive(Default)]
pub struct AddressVisitor;

impl<'b> Visitor<'b> for AddressVisitor {
	type Value = Address;

	fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
		formatter.write_str("an address")
	}

	fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: ::serde::de::Error {
		value.parse().map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
	}
}

pub mod vec {
	use serde::{Serialize, Serializer, Deserializer, Deserialize};
	use serde::de::Visitor;
	use keys::Address;
	use super::AddressVisitor;

	pub fn serialize<S>(addresses: &Vec<Address>, serializer: S) -> Result<S::Ok, S::Error> where S: Serializer {
		addresses.iter().map(|address| address.to_string()).collect::<Vec<_>>().serialize(serializer)
	}

	pub fn deserialize<'a, D>(deserializer: D) -> Result<Vec<Address>, D::Error> where D: Deserializer<'a> {
		<Vec<&'a str> as Deserialize>::deserialize(deserializer)?
			.into_iter()
			.map(|value| AddressVisitor::default().visit_str(value))
			.collect()
	}
}

#[cfg(test)]
mod tests {
	use serde_json;
	use keys::Address;
	use v1::types;

	#[derive(Debug, PartialEq, Serialize, Deserialize)]
	struct TestStruct {
		#[serde(with = "types::address")]
		address: Address,
	}

	impl TestStruct {
		fn new(address: Address) -> Self {
			TestStruct {
				address: address,
			}
		}
	}

	#[test]
	fn address_serialize() {
		let test = TestStruct::new("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into());
		assert_eq!(serde_json::to_string(&test).unwrap(), r#"{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"}"#);
	}

	#[test]
	fn address_deserialize() {
		let test = TestStruct::new("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into());
		assert_eq!(serde_json::from_str::<TestStruct>(r#"{"address":"1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"}"#).unwrap(), test);
	}
}
