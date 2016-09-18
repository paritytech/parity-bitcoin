use std::{io, cmp};
use byteorder::{LittleEndian, ReadBytesExt};
use compact_integer::CompactInteger;

pub fn deserialize<T>(buffer: &[u8]) -> Result<T, Error> where T: Deserializable {
	let mut reader = Reader::new(buffer);
	let result = try!(reader.read());
	if !reader.is_finished() {
		return Err(Error::UnreadData);
	}

	Ok(result)
}

#[derive(Debug, PartialEq)]
pub enum Error {
	MalformedData,
	UnexpectedEnd,
	UnreadData,
}

impl From<io::Error> for Error {
	fn from(_: io::Error) -> Self {
		Error::UnexpectedEnd
	}
}

pub trait Deserializable {
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized;
}

#[derive(Debug)]
pub struct Reader<'a> {
	buffer: &'a [u8],
	read: usize,
}

impl<'a> io::Read for Reader<'a> {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		let to_read = cmp::min(self.buffer.len() - self.read, buf.len());
		buf[0..to_read].copy_from_slice(&self.buffer[self.read..self.read + to_read]);
		self.read += to_read;
		Ok(to_read)
	}
}

impl<'a> Reader<'a> {
	pub fn new(buffer: &'a [u8]) -> Reader {
		Reader {
			buffer: buffer,
			read: 0,
		}
	}

	pub fn read<T>(&mut self) -> Result<T, Error> where T: Deserializable {
		T::deserialize(self)
	}

	pub fn read_slice(&mut self, len: usize) -> Result<&'a [u8], Error> {
		if self.read + len > self.buffer.len() {
			return Err(Error::UnexpectedEnd);
		}

		let result = &self.buffer[self.read..self.read + len];
		self.read += len;
		Ok(result)
	}

	pub fn read_list<T>(&mut self) -> Result<Vec<T>, Error> where T: Deserializable {
		let len: usize = try!(self.read::<CompactInteger>()).into();
		let mut result = vec![];

		for _ in 0..len {
			result.push(try!(self.read()));
		}

		Ok(result)
	}

	/// Returns true if reading is finished.
	pub fn is_finished(&self) -> bool {
		self.read == self.buffer.len()
	}
}

impl Deserializable for i32 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_i32::<LittleEndian>()))
	}
}

impl Deserializable for u8 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_u8()))
	}
}

impl Deserializable for u16 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_u16::<LittleEndian>()))
	}
}

impl Deserializable for u32 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_u32::<LittleEndian>()))
	}
}

impl Deserializable for u64 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_u64::<LittleEndian>()))
	}
}

#[cfg(test)]
mod test {
	use super::{Reader, Error};

	#[test]
	fn test_reader_read() {
		let buffer = vec![
			1,
			2, 0,
			3, 0, 0, 0,
			4, 0, 0, 0, 0, 0, 0, 0
		];

		let mut reader = Reader::new(&buffer);
		assert!(!reader.is_finished());
		assert_eq!(1u8, reader.read().unwrap());
		assert_eq!(2u16, reader.read().unwrap());
		assert_eq!(3u32, reader.read().unwrap());
		assert_eq!(4u64, reader.read().unwrap());
		assert!(reader.is_finished());
		assert_eq!(Error::UnexpectedEnd, reader.read::<u8>().unwrap_err());
	}
}
