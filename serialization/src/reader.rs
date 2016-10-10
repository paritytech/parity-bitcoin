use std::{io, cmp};
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
		let mut result = Vec::with_capacity(len);

		for _ in 0..len {
			result.push(try!(self.read()));
		}

		Ok(result)
	}

	pub fn read_list_max<T>(&mut self, max: usize) -> Result<Vec<T>, Error> where T: Deserializable {
		let len: usize = try!(self.read::<CompactInteger>()).into();
		if len > max {
			return Err(Error::MalformedData);
		}

		let mut result = Vec::with_capacity(len);

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
