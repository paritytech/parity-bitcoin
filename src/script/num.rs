//! Script numeric.
use std::ops;
use script::Error;

/// Numeric opcodes (OP_1ADD, etc) are restricted to operating on 4-byte integers.
/// The semantics are subtle, though: operands must be in the range [-2^31 +1...2^31 -1],
/// but results may overflow (and are valid as long as they are not used in a subsequent
/// numeric operation). CScriptNum enforces those semantics by storing results as
/// an int64 and allowing out-of-range values to be returned as a vector of bytes but
/// throwing an exception if arithmetic is done or the result is interpreted as an integer.
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Num {
	value: i64,
}

impl From<u8> for Num {
	fn from(i: u8) -> Self {
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

impl Num {
	pub fn from_slice(data: &[u8], require_minimal: bool, max_size: usize) -> Result<Self, Error> {
		if data.len() > max_size {
			return Err(Error::NumberOverflow);
		}

		if data.is_empty() {
			return Ok(0u8.into());
		}

		if require_minimal {
			// Check that the number is encoded with the minimum possible
			// number of bytes.
			//
			// If the most-significant-byte - excluding the sign bit - is zero
			// then we're not minimal. Note how this test also rejects the
			// negative-zero encoding, 0x80.
			if (data.last().unwrap() & 0x7f) == 0 {
				if data.len() <= 1 || (data[data.len() - 2] & 0x80) == 0 {
					return Err(Error::NumberNotMinimallyEncoded)
				}
			}
		}

		let mut result = 0i64;
		for i in 0..data.len() {
			result |= (data[i] as i64) << (8 * i);
		}

		// If the input vector's most significant byte is 0x80, remove it from
		// the result's msb and return a negative.
		if data.last().unwrap() & 0x80 != 0 {
			Ok((-(result & !(0x80i64 << (8 * (data.len() - 1))))).into())
		} else {
			Ok(result.into())
		}
	}

	pub fn to_vec(&self) -> Vec<u8> {
		if self.value == 0 {
			return vec![];
		}

		let mut result = vec![];
		let negative = self.value < 0;
		let mut absvalue = match negative {
			true => (-self.value) as u64,
			false => self.value as u64,
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
			match negative {
				true => result.push(0x80),
				false => result.push(0),
			}
		} else if negative {
			let rlen = result.len();
			result[rlen - 1] |= 0x80;
		}

		result
	}

	pub fn is_negative(&self) -> bool {
		self.value < 0
	}

	pub fn is_zero(&self) -> bool {
		self.value == 0
	}
}

impl ops::BitAnd for Num {
	type Output = Self;

	fn bitand(self, rhs: Self) -> Self::Output {
		(self.value & rhs.value).into()
	}
}
