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

impl From<i64> for Num {
	fn from(i: i64) -> Self {
		Num {
			value: i
		}
	}
}
