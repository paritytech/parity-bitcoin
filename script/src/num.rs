//! Script numeric

use std::ops;
use bytes::Bytes;
use Error;

/// Script numeric
///
/// Numeric opcodes (`OP_1ADD`, etc) are restricted to operating on 4-byte integers.
/// The semantics are subtle, though: operands must be in the range [-2^31 +1...2^31 -1],
/// but results may overflow (and are valid as long as they are not used in a subsequent
/// numeric operation). `CScriptNum` enforces those semantics by storing results as
/// an int64 and allowing out-of-range values to be returned as a vector of bytes but
/// throwing an exception if arithmetic is done or the result is interpreted as an integer.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct Num {
	value: i64,
}

impl From<bool> for Num {
	fn from(i: bool) -> Self {
		let v = if i { 1 } else { 0 };
		Num {
			value: v
		}
	}
}

impl From<u8> for Num {
	fn from(i: u8) -> Self {
		Num {
			value: i as i64
		}
	}
}

impl From<u32> for Num {
	fn from(i: u32) -> Self {
		Num {
			value: i as i64
		}
	}
}

impl From<usize> for Num {
	fn from(i: usize) -> Self {
		Num {
			value: i as i64
		}
	}
}

impl From<i32> for Num {
	fn from(i: i32) -> Self {
		Num {
			value: i as i64
		}
	}
}

impl From<i64> for Num {
	fn from(i: i64) -> Self {
		Num {
			value: i
		}
	}
}

impl From<Num> for i64 {
	fn from(n: Num) -> Self {
		n.value
	}
}

impl From<Num> for u32 {
	fn from(n: Num) -> Self {
		n.value as u32
	}
}

impl From<Num> for usize {
	fn from(n: Num) -> Self {
		n.value as usize
	}
}

impl Num {
	/// Reduce the data size to its minimal, and then try to convert it to a num.
	pub fn minimally_encode(data: &[u8], max_size: usize) -> Result<Self, Error> {
		match data.last() {
			None => Num::from_slice(data, true, max_size),
			Some(last) => {
				if *last != 0x00 && *last != 0x80 {
					return Num::from_slice(data, true, max_size);
				}

				if data.len() == 1 {
					return Num::from_slice(&[], true, max_size);
				}

				if data[data.len() - 2] & 0x80 == 0x80 {
					return Num::from_slice(data, true, max_size);
				}

				// We are not minimally encoded. Create a vector so that we can trim the result. The last byte is not included,
				// as we first trim all zeros. And then a conditional to decide what to do with the last byte.
				let mut data: Vec<u8> = data[0..(data.len()-1)].iter().cloned().rev().skip_while(|x| *x == 0x00).collect();
				data.reverse();

				if data.len() == 0 {
					// At this point, last is either equal to 0x00 or 0x80. The result is empty.
					return Num::from_slice(&[], true, max_size);
				}

				let second_last = *data.last().expect("vec emptiness is checked above; qed");
				if second_last & 0x80 == 0x80 {
					data.push(*last);
				} else {
					*data.last_mut().expect("vec emptiness is checked above; qed") |= *last
				}

				Num::from_slice(&data, true, max_size)
			}
		}
	}

	pub fn from_slice(data: &[u8], require_minimal: bool, max_size: usize) -> Result<Self, Error> {
		if data.len() > max_size {
			return Err(Error::NumberOverflow);
		}

		if data.is_empty() {
			return Ok(0u8.into());
		}

		// Check that the number is encoded with the minimum possible
		// number of bytes.
		//
		// If the most-significant-byte - excluding the sign bit - is zero
		// then we're not minimal. Note how this test also rejects the
		// negative-zero encoding, 0x80.
		if require_minimal &&
			(data.last().unwrap() & 0x7f) == 0 &&
			(data.len() <= 1 || (data[data.len() - 2] & 0x80) == 0) {
			return Err(Error::NumberNotMinimallyEncoded)
		}

		let mut result = 0i64;
		for (i, item) in data.iter().enumerate() {
			result |= (*item as i64) << (8 * i);
		}

		// If the input vector's most significant byte is 0x80, remove it from
		// the result's msb and return a negative.
		if data.last().unwrap() & 0x80 != 0 {
			Ok((-(result & !(0x80i64 << (8 * (data.len() - 1))))).into())
		} else {
			Ok(result.into())
		}
	}

	pub fn to_bytes(&self) -> Bytes {
		if self.value == 0 {
			return Bytes::default();
		}

		let mut result = vec![];
		let negative = self.value < 0;
		let mut absvalue = if negative {
			(-self.value) as u64
		} else {
			self.value as u64
		};

		while absvalue > 0 {
			result.push(absvalue as u8 & 0xff);
			absvalue >>= 8;
		}

		//    - If the most significant byte is >= 0x80 and the value is positive, push a
		//    new zero-byte to make the significant byte < 0x80 again.

		//    - If the most significant byte is >= 0x80 and the value is negative, push a
		//    new 0x80 byte that will be popped off when converting to an integral.

		//    - If the most significant byte is < 0x80 and the value is negative, add
		//    0x80 to it, since it will be subtracted and interpreted as a negative when
		//    converting to an integral.

		if result[result.len() - 1] & 0x80 != 0 {
			if negative {
				result.push(0x80);
			} else {
				result.push(0);
			}
		} else if negative {
			let rlen = result.len();
			result[rlen - 1] |= 0x80;
		}

		result.into()
	}

	pub fn is_negative(&self) -> bool {
		self.value < 0
	}

	pub fn is_zero(&self) -> bool {
		self.value == 0
	}

	pub fn abs(&self) -> Num {
		if self.value < 0 {
			(-self.value).into()
		} else {
			self.value.into()
		}
	}
}

impl ops::BitAnd for Num {
	type Output = Self;

	fn bitand(self, rhs: Self) -> Self::Output {
		(self.value & rhs.value).into()
	}
}

impl ops::Add for Num {
	type Output = Self;

	fn add(self, rhs: Self) -> Self::Output {
		(self.value + rhs.value).into()
	}
}

impl ops::Sub for Num {
	type Output = Self;

	fn sub(self, rhs: Self) -> Self::Output {
		(self.value - rhs.value).into()
	}
}

impl ops::Neg for Num {
	type Output = Self;

	fn neg(self) -> Self::Output {
		(-self.value).into()
	}
}

impl ops::Div for Num {
	type Output = Self;

	fn div(self, rhs: Self) -> Self::Output {
		(self.value / rhs.value).into()
	}
}

impl ops::Rem for Num {
	type Output = Self;

	fn rem(self, rhs: Self) -> Self::Output {
		(self.value % rhs.value).into()
	}
}
