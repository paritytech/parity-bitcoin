//! Compact representation of `U256`

use bigint::{U256, Uint};

/// Compact representation of `U256`
#[derive(Debug, PartialEq, Clone, Copy)]
pub struct Compact(u32);

impl From<u32> for Compact {
	fn from(u: u32) -> Self {
		Compact(u)
	}
}

impl From<Compact> for u32 {
	fn from(c: Compact) -> Self {
		c.0
	}
}

impl From<U256> for Compact {
	fn from(u: U256) -> Self {
		Compact::from_u256(u)
	}
}

impl From<Compact> for U256 {
	fn from(c: Compact) -> Self {
		// ignore overflows and negative values
		c.to_u256().unwrap_or_else(|x| x)
	}
}

impl Compact {
	pub fn new(u: u32) -> Self {
		Compact(u)
	}

	pub fn max_value() -> Self {
		U256::max_value().into()
	}

	/// Computes the target [0, T] that a blockhash must land in to be valid
	/// Returns value in error, if there is an overflow or its negative value
	pub fn to_u256(&self) -> Result<U256, U256> {
		let size = self.0 >> 24;
		let mut word = self.0 & 0x007fffff;

		let result = if size <= 3 {
			word >>= 8 * (3 - size as usize);
			word.into()
		} else {
			U256::from(word) << (8 * (size as usize - 3))
		};

		let is_negative = word != 0 && (self.0 & 0x00800000) != 0;
		let is_overflow = (word != 0 && size > 34) ||
				(word > 0xff && size > 33) ||
				(word > 0xffff && size > 32);

		if is_negative || is_overflow {
			Err(result)
		} else {
			Ok(result)
		}
	}

	pub fn from_u256(val: U256) -> Self {
		let mut size = (val.bits() + 7) / 8;
		let mut compact = if size <= 3 {
			(val.low_u64() << (8 * (3 - size))) as u32
		} else {
			let bn = val >> (8 * (size - 3));
			bn.low_u32()
		};

		if (compact & 0x00800000) != 0 {
			compact >>= 8;
			size += 1;
		}

		assert!((compact & !0x007fffff) == 0);
		assert!(size < 256);
		Compact(compact | (size << 24) as u32)
	}

	pub fn to_f64(&self) -> f64 {
    	let max_body = f64::from(0x00ffff).ln();
    	let scaland = f64::from(256).ln();
    	let ln1 = f64::from(self.0 & 0x00ffffff).ln();
    	let s1 = scaland * f64::from(0x1d - ((self.0 & 0xff000000) >> 24));
		(max_body - ln1 + s1).exp()
	}
}

#[cfg(test)]
mod tests {
	use bigint::{U256, Uint};
	use super::Compact;

	#[test]
	fn test_compact_to_u256() {
		assert_eq!(Compact::new(0x01003456).to_u256(), Ok(0.into()));
		assert_eq!(Compact::new(0x01123456).to_u256(), Ok(0x12.into()));
		assert_eq!(Compact::new(0x02008000).to_u256(), Ok(0x80.into()));
		assert_eq!(Compact::new(0x05009234).to_u256(), Ok(0x92340000u64.into()));
		// negative -0x12345600
		assert!(Compact::new(0x04923456).to_u256().is_err());
		assert_eq!(Compact::new(0x04123456).to_u256(), Ok(0x12345600u64.into()));
	}

	#[test]
	fn test_from_u256() {
		let test1 = U256::from(1000u64);
		assert_eq!(Compact::new(0x0203e800), Compact::from_u256(test1));

		let test2 = U256::from(2).pow(U256::from(256-32)) - U256::from(1);
		assert_eq!(Compact::new(0x1d00ffff), Compact::from_u256(test2));
	}

	#[test]
	fn test_compact_to_from_u256() {
		// TODO: it does not work both ways for small values... check why
		let compact = Compact::new(0x1d00ffff);
		let compact2 = Compact::from_u256(compact.to_u256().unwrap());
		assert_eq!(compact, compact2);

		let compact = Compact::new(0x05009234);
		let compact2 = Compact::from_u256(compact.to_u256().unwrap());
		assert_eq!(compact, compact2);
	}

	#[test]
	fn difficulty() {
		let nbits = Compact::new(0x1b0404cb);
		assert_eq!(nbits.to_f64(), 16307.420938523994f64);
	}
}
