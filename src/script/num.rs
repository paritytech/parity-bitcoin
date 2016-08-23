//! Script numeric.

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

impl From<i64> for Num {
	fn from(i: i64) -> Self {
		Num {
			value: i
		}
	}
}

impl Num {
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
}
