use uint::U256;

pub const MAX_NBITS_MAINNET: u32 = 0x1d00ffff;
pub const MAX_NBITS_TESTNET: u32 = 0x1d00ffff;
pub const MAX_NBITS_REGTEST: u32 = 0x207fffff;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct NBits(u32);

impl NBits {
	pub fn new(u: u32) -> Self {
		NBits(u)
	}

	/// Computes the target [0, T] that a blockhash must land in to be valid
	/// Returns None, if there is an overflow or its negative value
	pub fn target(&self) -> Option<U256> {
		let size = self.0 >> 24;
		let mut word = self.0 & 0x007fffff;

		let result = if size <= 3 {
			word = word >> (8 * (3 - size as usize));
			word.into()
		} else {
			U256::from(word) << (8 * (size as usize - 3))
		};

		let is_negative = word != 0 && (self.0 & 0x00800000) != 0;
		let is_overflow = (word != 0 && size > 34) ||
				(word > 0xff && size > 33) ||
				(word > 0xffff && size > 32);

		if is_negative || is_overflow {
			None
		} else {
			Some(result)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::NBits;

	#[test]
	fn test_basic_nbits_target() {
		assert_eq!(NBits::new(0x01003456).target(), Some(0.into()));
		assert_eq!(NBits::new(0x01123456).target(), Some(0x12.into()));
		assert_eq!(NBits::new(0x02008000).target(), Some(0x80.into()));
		assert_eq!(NBits::new(0x05009234).target(), Some(0x92340000u64.into()));
		// negative -0x12345600
		assert_eq!(NBits::new(0x04923456).target(), None);
		assert_eq!(NBits::new(0x04123456).target(), Some(0x12345600u64.into()));
	}
}
