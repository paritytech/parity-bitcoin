//! Fixed-size hashes

use std::{fmt, ops, cmp, str};
use hex::{ToHex, FromHex, FromHexError};
use std::hash::{Hash, Hasher};

macro_rules! impl_hash {
	($name: ident, $size: expr) => {
		#[repr(C)]
		#[derive(Copy)]
		pub struct $name([u8; $size]);

		impl Default for $name {
			fn default() -> Self {
				$name([0u8; $size])
			}
		}

		impl AsRef<$name> for $name {
			fn as_ref(&self) -> &$name {
				self
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

		impl<'a> From<&'a [u8]> for $name {
			fn from(slc: &[u8]) -> Self {
				let mut inner = [0u8; $size];
				inner[..].clone_from_slice(&slc[0..$size]);
				$name(inner)
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
				result.0[0] = v;
				result
			}
		}

		impl str::FromStr for $name {
			type Err = FromHexError;

			fn from_str(s: &str) -> Result<Self, Self::Err> {
				let vec: Vec<u8> = try!(s.from_hex());
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
				f.write_str(&self.0.to_hex::<String>())
			}
		}

		impl fmt::Display for $name {
			fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
				f.write_str(&self.0.to_hex::<String>())
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

		impl cmp::PartialOrd for $name {
			fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
				let self_ref: &[u8] = &self.0;
				let other_ref: &[u8] = &other.0;
				self_ref.partial_cmp(other_ref)
			}
		}


		impl Hash for $name {
			fn hash<H>(&self, state: &mut H) where H: Hasher {
				state.write(&self.0);
				state.finish();
			}
		}

		impl Eq for $name { }

		impl $name {
			pub fn take(self) -> [u8; $size] {
				self.0
			}

			pub fn reversed(&self) -> Self {
				let mut result = self.clone();
				result.reverse();
				result
			}

			pub fn size() -> usize {
				$size
			}

			pub fn is_zero(&self) -> bool {
				self.0.iter().all(|b| *b == 0)
			}
		}
	}
}

impl_hash!(H32, 4);
impl_hash!(H48, 6);
impl_hash!(H96, 12);
impl_hash!(H160, 20);
impl_hash!(H256, 32);
impl_hash!(H264, 33);
impl_hash!(H512, 64);
impl_hash!(H520, 65);

known_heap_size!(0, H32, H48, H96, H160, H256, H264, H512, H520);

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
