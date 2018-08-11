use std::fmt;
use std::str::FromStr;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
use serde;
use serde::de::Unexpected;
use hex::{ToHex, FromHex};
use primitives::hash::H256 as GlobalH256;
use primitives::hash::H160 as GlobalH160;

macro_rules! impl_hash {
	($name: ident, $other: ident, $size: expr) => {
		/// Hash serialization
		#[derive(Eq)]
		pub struct $name([u8; $size]);

		impl Default for $name {
			fn default() -> Self {
				$name([0; $size])
			}
		}

		impl fmt::Debug for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
				write!(f, "{}", $other::from(self.0.clone()).to_hex::<String>())
			}
		}

		impl<T> From<T> for $name where $other: From<T> {
			fn from(o: T) -> Self {
				$name($other::from(o).take())
			}
		}

		impl FromStr for $name {
			type Err = <$other as FromStr>::Err;

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				let other = try!($other::from_str(s));
				Ok($name(other.take()))
			}
		}

		impl Into<$other> for $name {
			fn into(self) -> $other {
				$other::from(self.0)
			}
		}

		impl PartialEq for $name {
			fn eq(&self, other: &Self) -> bool {
				let self_ref: &[u8] = &self.0;
				let other_ref: &[u8] = &other.0;
				self_ref == other_ref
			}
		}

		impl PartialOrd for $name {
			fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
				let self_ref: &[u8] = &self.0;
				let other_ref: &[u8] = &other.0;
				self_ref.partial_cmp(other_ref)
			}
		}

		impl Ord for $name {
			fn cmp(&self, other: &Self) -> Ordering {
				let self_ref: &[u8] = &self.0;
				let other_ref: &[u8] = &other.0;
				self_ref.cmp(other_ref)
			}
		}

		impl Hash for $name {
			fn hash<H>(&self, state: &mut H) where H: Hasher {
				$other::from(self.0.clone()).hash(state)
			}
		}

		impl Clone for $name {
			fn clone(&self) -> Self {
				let mut r = [0; $size];
				r.copy_from_slice(&self.0);
				$name(r)
			}
		}

		impl serde::Serialize for $name {
			fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
			where S: serde::Serializer {
				let mut hex = String::new();
				hex.push_str(&$other::from(self.0.clone()).to_hex::<String>());
				serializer.serialize_str(&hex)
			}
		}

		impl<'a> serde::Deserialize<'a> for $name {
			fn deserialize<D>(deserializer: D) -> Result<$name, D::Error> where D: serde::Deserializer<'a> {
				struct HashVisitor;

				impl<'b> serde::de::Visitor<'b> for HashVisitor {
					type Value = $name;

					fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
						formatter.write_str("a hash string")
					}

					fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> where E: serde::de::Error {

						if value.len() != $size * 2 {
							return Err(E::invalid_value(Unexpected::Str(value), &self))
						}

						match value[..].from_hex::<Vec<u8>>() {
							Ok(ref v) => {
								let mut result = [0u8; $size];
								result.copy_from_slice(v);
								Ok($name($other::from(result).take()))
							},
							_ => Err(E::invalid_value(Unexpected::Str(value), &self))

						}
					}

					fn visit_string<E>(self, value: String) -> Result<Self::Value, E> where E: serde::de::Error {
						self.visit_str(value.as_ref())
					}
				}

				deserializer.deserialize_identifier(HashVisitor)
			}
		}
	}
}

impl_hash!(H256, GlobalH256, 32);
impl_hash!(H160, GlobalH160, 20);

impl H256 {
	pub fn reversed(&self) -> Self {
		let mut result = self.clone();
		result.0.reverse();
		result
	}
}

#[cfg(test)]
mod tests {
	use super::H256;
	use primitives::hash::H256 as GlobalH256;
	use std::str::FromStr;

	#[test]
	fn hash_debug() {
		let str_reversed = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
		let reversed_hash = H256::from(str_reversed);
		let debug_result = format!("{:?}", reversed_hash);
		assert_eq!(debug_result, str_reversed);
	}

	#[test]
	fn hash_from_str() {
		let str_reversed = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
		match H256::from_str(str_reversed) {
			Ok(reversed_hash) => assert_eq!(format!("{:?}", reversed_hash), str_reversed),
			_ => panic!("unexpected"),
		}

		let str_reversed = "XXXYYY";
		match H256::from_str(str_reversed) {
			Ok(_) => panic!("unexpected"),
			_ => (),
		}
	}

	#[test]
	fn hash_to_global_hash() {
		let str_reversed = "00000000839a8e6886ab5951d76f411475428afc90947ee320161bbf18eb6048";
		let reversed_hash = H256::from(str_reversed);
		let global_hash = GlobalH256::from(str_reversed);
		let global_converted: GlobalH256 = reversed_hash.into();
		assert_eq!(global_converted, global_hash);
	}
}
