use std::{io, marker};
use compact_integer::CompactInteger;

pub fn deserialize<R, T>(buffer: R) -> Result<T, Error> where R: io::Read, T: Deserializable {
	let mut reader = Reader::from_read(buffer);
	let result = try!(reader.read());

	match reader.is_finished() {
		false => Err(Error::UnreadData),
		true => Ok(result),
	}
}

pub fn deserialize_iterator<R, T>(buffer: R) -> ReadIterator<R, T> where R: io::Read, T: Deserializable {
	ReadIterator {
		reader: Reader::from_read(buffer),
		iter_type: marker::PhantomData,
	}
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
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, Error> where Self: Sized, T: io::Read;
}

/// Bitcoin structures reader.
#[derive(Debug)]
pub struct Reader<T> {
	buffer: T,
	peeked: Option<u8>,
}

impl<'a> Reader<&'a [u8]> {
	/// Convenient way of creating for slice of bytes
	pub fn new(buffer: &'a [u8]) -> Self {
		Reader {
			buffer: buffer,
			peeked: None,
		}
	}
}

impl<T> io::Read for Reader<T> where T: io::Read {
	fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
		// most of the times, there will be nothing in peeked,
		// so to make it as efficient as possible, check it
		// only once
		match self.peeked.take() {
			None => io::Read::read(&mut self.buffer, buf),
			Some(peeked) if buf.is_empty() => {
				self.peeked = Some(peeked);
				Ok(0)
			},
			Some(peeked) => {
				buf[0] = peeked;
				io::Read::read(&mut self.buffer, &mut buf[1..]).map(|x| x + 1)
			},
		}
	}
}

impl<R> Reader<R> where R: io::Read {
	pub fn from_read(read: R) -> Self {
		Reader {
			buffer: read,
			peeked: None,
		}
	}

	pub fn read<T>(&mut self) -> Result<T, Error> where T: Deserializable {
		T::deserialize(self)
	}

	pub fn read_slice(&mut self, bytes: &mut [u8]) -> Result<(), Error> {
		io::Read::read_exact(self, bytes).map_err(|_| Error::UnexpectedEnd)
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

	pub fn is_finished(&mut self) -> bool {
		if self.peeked.is_some() {
			return false;
		}

		let peek: &mut [u8] = &mut [0u8];
		match self.read_slice(peek) {
			Ok(_) => {
				self.peeked = Some(peek[0]);
				false
			},
			Err(_) => true,
		}
	}
}

/// Should be used to iterate over structures of the same type
pub struct ReadIterator<R, T> {
	reader: Reader<R>,
	iter_type: marker::PhantomData<T>,
}

impl<R, T> Iterator for ReadIterator<R, T> where R: io::Read, T: Deserializable {
	type Item = Result<T, Error>;

	fn next(&mut self) -> Option<Self::Item> {
		match self.reader.is_finished() {
			true => None,
			false => Some(self.reader.read())
		}
	}
}
