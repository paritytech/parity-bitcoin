
use byteorder::{LittleEndian, ReadBytesExt};

#[derive(Debug, PartialEq)]
pub enum Error {
	MalformedData,
	UnexpectedEnd,
}

pub trait Deserializable {
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized;
}

pub struct Reader<'a> {
	buffer: &'a [u8],
	read: usize,
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

	pub fn read_bytes(&mut self, len: usize) -> Result<&'a [u8], Error> {
		if self.read + len > self.buffer.len() {
			return Err(Error::UnexpectedEnd);
		}

		let result = &self.buffer[self.read..self.read + len];
		self.read += len;
		Ok(result)
	}
}

impl Deserializable for i32 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_bytes(4)).read_i32::<LittleEndian>().unwrap())
	}
}

impl Deserializable for u8 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_bytes(1)).read_u8().unwrap())
	}
}

impl Deserializable for u16 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_bytes(2)).read_u16::<LittleEndian>().unwrap())
	}
}

impl Deserializable for u32 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_bytes(4)).read_u32::<LittleEndian>().unwrap())
	}
}

impl Deserializable for u64 {
	#[inline]
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		Ok(try!(reader.read_bytes(8)).read_u64::<LittleEndian>().unwrap())
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
		assert_eq!(1u8, reader.read().unwrap());
		assert_eq!(2u16, reader.read().unwrap());
		assert_eq!(3u32, reader.read().unwrap());
		assert_eq!(4u64, reader.read().unwrap());
		assert_eq!(Error::UnexpectedEnd, reader.read::<u8>().unwrap_err());
	}
}
