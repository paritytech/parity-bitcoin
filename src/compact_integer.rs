//! A type of variable-length integer commonly used in the Bitcoin P2P protocol and Bitcoin serialized data structures.
//! https://bitcoin.org/en/developer-reference#compactsize-unsigned-integers

use stream::{Serializable, Stream};

/// A type of variable-length integer commonly used in the Bitcoin P2P protocol and Bitcoin serialized data structures.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct CompactInteger(u64);

impl From<CompactInteger> for u64 {
	fn from(i: CompactInteger) -> Self {
		i.0
	}
}

impl From<usize> for CompactInteger {
	fn from(i: usize) -> Self {
		CompactInteger(i as u64)
	}
}

impl From<u64> for CompactInteger {
	fn from(i: u64) -> Self {
		CompactInteger(i)
	}
}

impl Serializable for CompactInteger {
	fn serialize(&self, stream: &mut Stream) {
		match self.0 {
			0...0xfc => {
				stream.append(&(self.0 as u8));
			},
			0xfd...0xffff => {
				stream
					.append(&0xfdu8)
					.append(&(self.0 as u16));
			},
			0x10000...0xffff_ffff => {
				stream
					.append(&0xfeu8)
					.append(&(self.0 as u32));
			},
			_ => {
				stream
					.append(&0xffu8)
					.append(&self.0);
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use stream::Stream;
	use super::CompactInteger;

	#[test]
	fn test_compact_integer_stream() {
		let mut stream = Stream::default();

		stream
			.append(&CompactInteger::from(0u64))
			.append(&CompactInteger::from(0xfcu64))
			.append(&CompactInteger::from(0xfdu64))
			.append(&CompactInteger::from(0xffffu64))
			.append(&CompactInteger::from(0x10000u64))
			.append(&CompactInteger::from(0xffff_ffffu64))
			.append(&CompactInteger::from(0x1_0000_0000u64));

		let expected = vec![
			0,
			0xfc,
			0xfd, 0xfd, 0x00,
			0xfd, 0xff, 0xff,
			0xfe, 0x00, 0x00, 0x01, 0x00,
			0xfe, 0xff, 0xff, 0xff, 0xff,
			0xff, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
		];

		assert_eq!(stream.out(), expected);
	}
}
