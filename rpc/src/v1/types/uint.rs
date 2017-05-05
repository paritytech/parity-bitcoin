use std::fmt;
use std::str::FromStr;
use serde;
use serde::de::Unexpected;
use primitives::bigint::{U256 as GlobalU256, Uint};

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
			fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error> where S: serde::Serializer {
				let as_hex = format!("{}", self.0.to_hex());
				serializer.serialize_str(&as_hex)
			}
		}

		impl<'a> serde::Deserialize<'a> for $name {
			fn deserialize<D>(deserializer: D) -> Result<$name, D::Error> where D: serde::Deserializer<'a> {
				struct UintVisitor;

				impl<'b> serde::de::Visitor<'b> for UintVisitor {
					type Value = $name;

					fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
						formatter.write_str("an integer represented in hex string")
					}

					fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: serde::de::Error {
						if value.len() > $size * 16 {
							return Err(E::invalid_value(Unexpected::Str(value), &self))
						}

						$other::from_str(value).map($name).map_err(|_| E::invalid_value(Unexpected::Str(value), &self))
					}

					fn visit_string<E>(self, value: String) -> Result<Self::Value, E> where E: serde::de::Error {
						self.visit_str(&value)
					}
				}

				deserializer.deserialize_identifier(UintVisitor)
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
