use std::ops;
use std::str::FromStr;
use serde::{Serialize, Deserialize, Serializer, Deserializer};
use global_script::ScriptAddress;
use keys::Address as GlobalAddress;
use keys::Network as KeysNetwork;
use network::Magic;

/// Bitcoin address
#[derive(Debug, PartialEq)]
pub struct Address(GlobalAddress);

impl Address {
	pub fn new(network: Magic, address: ScriptAddress) -> Self {
		Address(GlobalAddress {
			network: match network {
				Magic::Mainnet => KeysNetwork::Mainnet,
				// there's no correct choices for Regtests && Other networks
				// => let's just make Testnet key
				_ => KeysNetwork::Testnet,
			},
			hash: address.hash,
			kind: address.kind,
		})
	}

	pub fn deserialize_from_string<E>(value: &str) -> Result<Address, E> where E: ::serde::de::Error {
		GlobalAddress::from_str(value)
			.map_err(|err| E::invalid_value(&format!("error {} parsing address {}", err, value)))
			.map(|address| Address(address))
	}
}

impl Serialize for Address {
	fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: Serializer {
		serializer.serialize_str(&self.0.to_string())
	}
}

impl Deserialize for Address {
	fn deserialize<D>(deserializer: &mut D) -> Result<Self, D::Error> where D: Deserializer {
		use serde::de::Visitor;

		struct AddressVisitor;

		impl Visitor for AddressVisitor {
			type Value = Address;

			fn visit_str<E>(&mut self, value: &str) -> Result<Address, E> where E: ::serde::de::Error {
				Address::deserialize_from_string(value)
			}
		}

		deserializer.deserialize(AddressVisitor)
	}
}

impl<T> From<T> for Address where GlobalAddress: From<T> {
	fn from(o: T) -> Self {
		Address(GlobalAddress::from(o))
	}
}

impl ops::Deref for Address {
	type Target = GlobalAddress;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

#[cfg(test)]
mod tests {
	use serde_json;
	use super::Address;

	#[test]
	fn address_serialize() {
		let address: Address = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into();
		assert_eq!(serde_json::to_string(&address).unwrap(), r#""1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa""#);
	}

	#[test]
	fn address_deserialize() {
		let address: Address = "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".into();
		assert_eq!(serde_json::from_str::<Address>(r#""1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa""#).unwrap(), address);
		assert!(serde_json::from_str::<Address>(r#""DEADBEEF""#).is_err());
	}
}
