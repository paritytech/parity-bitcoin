//! Variable-length integer commonly used in the Bitcoin [P2P protocol](https://bitcoin.org/en/developer-reference#compactsize-unsigned-integers)

use std::{fmt, io};
use {
	Serializable, Stream,
	Deserializable, Reader, Error as ReaderError
};

/// A type of variable-length integer commonly used in the Bitcoin P2P protocol and Bitcoin serialized data structures.
#[derive(Default, Debug, Clone, Copy, PartialEq)]
pub struct CompactInteger(u64);

impl fmt::Display for CompactInteger {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.fmt(f)
	}
}

impl From<CompactInteger> for usize {
	fn from(i: CompactInteger) -> Self {
		i.0 as usize
	}
}

impl From<CompactInteger> for u64 {
	fn from(i: CompactInteger) -> Self {
		i.0
	}
}

impl From<u8> for CompactInteger {
	fn from(i: u8) -> Self {
		CompactInteger(i as u64)
	}
}

impl From<u16> for CompactInteger {
	fn from(i: u16) -> Self {
		CompactInteger(i as u64)
	}
}

impl From<u32> for CompactInteger {
	fn from(i: u32) -> Self {
		CompactInteger(i as u64)
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
			0..=0xfc => {
				stream.append(&(self.0 as u8));
			},
			0xfd..=0xffff => {
				stream
					.append(&0xfdu8)
					.append(&(self.0 as u16));
			},
			0x10000..=0xffff_ffff => {
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

	fn serialized_size(&self) -> usize {
		match self.0 {
			0..=0xfc => 1,
			0xfd..=0xffff => 3,
			0x10000..=0xffff_ffff => 5,
			_ => 9,
		}
	}
}

impl Deserializable for CompactInteger {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = match try!(reader.read::<u8>()) {
			i @ 0..=0xfc => i.into(),
			0xfd => try!(reader.read::<u16>()).into(),
			0xfe => try!(reader.read::<u32>()).into(),
			_ => try!(reader.read::<u64>()).into(),
		};

		Ok(result)
	}
}

#[cfg(test)]
mod tests {
	use {Reader, Error as ReaderError, Stream};
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
		].into();

		assert_eq!(stream.out(), expected);
	}

	#[test]
	fn test_compact_integer_reader() {
		let buffer = vec![
			0,
			0xfc,
			0xfd, 0xfd, 0x00,
			0xfd, 0xff, 0xff,
			0xfe, 0x00, 0x00, 0x01, 0x00,
			0xfe, 0xff, 0xff, 0xff, 0xff,
			0xff, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00,
		];

		let mut reader = Reader::new(&buffer);
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0u64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0xfcu64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0xfdu64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0xffffu64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0x10000u64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0xffff_ffffu64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap(), 0x1_0000_0000u64.into());
		assert_eq!(reader.read::<CompactInteger>().unwrap_err(), ReaderError::UnexpectedEnd);
	}
}
