use std::str::FromStr;
use serde;
use primitives::uint::U256 as GlobalU256;

macro_rules! impl_uint {
	($name: ident, $other: ident, $size: expr) => {
		/// Uint serialization.
		#[derive(Debug, Default, Clone, Copy, PartialEq, Hash)]
		pub struct $name($other);

		impl Eq for $name { }

		impl<T> From<T> for $name where $other: From<T> {
			fn from(o: T) -> Self {
				$name($other::from(o))
			}
		}

		impl FromStr for $name {
			type Err = <$other as FromStr>::Err;

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				$other::from_str(s).map($name)
			}
		}

		impl Into<$other> for $name {
			fn into(self) -> $other {
				self.0
			}
		}

		impl serde::Serialize for $name {
			fn serialize<S>(&self, serializer: &mut S) -> Result<(), S::Error> where S: serde::Serializer {
				let as_hex = format!("{:x}", self.0);
				serializer.serialize_str(&as_hex)
			}
		}

		impl serde::Deserialize for $name {
			fn deserialize<D>(deserializer: &mut D) -> Result<$name, D::Error> where D: serde::Deserializer {
				struct UintVisitor;

				impl serde::de::Visitor for UintVisitor {
					type Value = $name;

					fn visit_str<E>(&mut self, value: &str) -> Result<Self::Value, E> where E: serde::Error {
						if value.len() > $size * 16 {
							return Err(serde::Error::custom("Invalid length."));
						}

						$other::from_str(value).map($name).map_err(|_| serde::Error::custom("Invalid hex value."))
					}

					fn visit_string<E>(&mut self, value: String) -> Result<Self::Value, E> where E: serde::Error {
						self.visit_str(&value)
					}
				}

				deserializer.deserialize(UintVisitor)
			}
		}
	}
}

impl_uint!(U256, GlobalU256, 4);


#[cfg(test)]
mod tests {
	use super::U256;
	use serde_json;

	#[test]
	fn u256_serialize() {
		let u256 = U256::from(256);
		let serialized = serde_json::to_string(&u256).unwrap();
		assert_eq!(serialized, r#""100""#);
	}

	#[test]
	fn u256_deserialize() {
		let u256 = U256::from(256);
		let deserialized = serde_json::from_str::<U256>(r#""100""#).unwrap();
		assert_eq!(deserialized, u256);
	}
}
