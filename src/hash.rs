use std::{fmt, ops, cmp, str};
use hex::{ToHex, FromHex, FromHexError};
use ser::{Stream, Serializable, Reader, Deserializable, Error as ReaderError};

macro_rules! impl_hash {
	($name: ident, $size: expr) => {
		#[repr(C)]
		pub struct $name([u8; $size]);

		impl Default for $name {
			fn default() -> Self {
				$name([0u8; $size])
			}
		}

		impl Clone for $name {
			fn clone(&self) -> Self {
				let mut result = Self::default();
				result.copy_from_slice(&self.0);
				result
			}
		}

		impl From<[u8; $size]> for $name {
			fn from(h: [u8; $size]) -> Self {
				$name(h)
			}
		}

		impl From<$name> for [u8; $size] {
			fn from(h: $name) -> Self {
				h.0
			}
		}

		impl From<&'static str> for $name {
			fn from(s: &'static str) -> Self {
				s.parse().unwrap()
			}
		}

		impl From<u8> for $name {
			fn from(v: u8) -> Self {
				let mut result = Self::default();
				result.0[$size - 1] = v;
				result
			}
		}

		impl str::FromStr for $name {
			type Err = FromHexError;

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				let vec = try!(s.from_hex());
				match vec.len() {
					$size => {
						let mut result = [0u8; $size];
						result.copy_from_slice(&vec);
						Ok($name(result))
					},
					_ => Err(FromHexError::InvalidHexLength)
				}
			}
		}

		impl fmt::Debug for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str(&self.0.to_hex())
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str(&self.0.to_hex())
			}
		}

		impl ops::Deref for $name {
			type Target = [u8; $size];

			fn deref(&self) -> &Self::Target {
				&self.0
			}
		}

		impl ops::DerefMut for $name {
			fn deref_mut(&mut self) -> &mut Self::Target {
				&mut self.0
			}
		}

		impl cmp::PartialEq for $name {
			fn eq(&self, other: &Self) -> bool {
				let self_ref: &[u8] = &self.0;
				let other_ref: &[u8] = &other.0;
				self_ref == other_ref
			}
		}

		// TODO: move to ser module

		impl Serializable for $name {
			fn serialize(&self, stream: &mut Stream) {
				stream.append_slice(&self.0);
			}
		}

		impl Deserializable for $name {
			fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
				let slice = try!(reader.read_slice($size));
				let mut result = Self::default();
				result.copy_from_slice(slice);
				Ok(result)
			}
		}

		impl $name {
			pub fn reversed(&self) -> Self {
				let mut result = self.clone();
				result.reverse();
				result
			}
		}
	}
}

impl_hash!(H160, 20);
impl_hash!(H256, 32);
impl_hash!(H264, 33);
impl_hash!(H512, 64);
impl_hash!(H520, 65);

impl H256 {
	#[inline]
	pub fn from_reversed_str(s: &'static str) -> Self {
		H256::from(s).reversed()
	}

	#[inline]
	pub fn to_reversed_str(&self) -> String {
		self.reversed().to_string()
	}
}
