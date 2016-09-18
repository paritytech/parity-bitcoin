//! Stream used for serialization.
use std::io::{self, Write};
use byteorder::{LittleEndian, WriteBytesExt};
use primitives::Bytes;
use ser::compact_integer::CompactInteger;

pub fn serialize(t: &Serializable) -> Vec<u8> {
	let mut stream = Stream::default();
	stream.append(t);
	stream.out()
}

pub trait Serializable {
	/// Serialize the struct and appends it to the end of stream.
	fn serialize(&self, s: &mut Stream);
}

/// Stream used for serialization.
#[derive(Default)]
pub struct Stream {
	buffer: Vec<u8>,
}

impl Stream {
	/// Serializes the struct and appends it to the end of stream.
	pub fn append(&mut self, t: &Serializable) -> &mut Self {
		t.serialize(self);
		self
	}

	/// Appends raw bytes to the end of the stream.
	pub fn append_slice(&mut self, bytes: &[u8]) -> &mut Self {
		// discard error for now, since we write to simple vector
		self.buffer.write(bytes).unwrap();
		self
	}

	/// Appends a list of serializable structs to the end of the stream.
	pub fn append_list<T>(&mut self, t: &[T]) -> &mut Self where T: Serializable {
		CompactInteger::from(t.len()).serialize(self);
		for i in t {
			i.serialize(self);
		}
		self
	}

	/// Full stream.
	pub fn out(self) -> Vec<u8> {
		self.buffer
	}
}

impl Write for Stream {
	#[inline]
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
		self.buffer.write(buf)
	}

	#[inline]
	fn flush(&mut self) -> Result<(), io::Error> {
		self.buffer.flush()
	}
}

impl Serializable for i32 {
	#[inline]
	fn serialize(&self, s: &mut Stream) {
		s.write_i32::<LittleEndian>(*self).unwrap();
	}
}

impl Serializable for u8 {
	#[inline]
	fn serialize(&self, s: &mut Stream) {
		s.write_u8(*self).unwrap();
	}
}

impl Serializable for u16 {
	#[inline]
	fn serialize(&self, s: &mut Stream) {
		s.write_u16::<LittleEndian>(*self).unwrap();
	}
}

impl Serializable for u32 {
	#[inline]
	fn serialize(&self, s: &mut Stream) {
		s.write_u32::<LittleEndian>(*self).unwrap();
	}
}

impl Serializable for u64 {
	#[inline]
	fn serialize(&self, s: &mut Stream) {
		s.write_u64::<LittleEndian>(*self).unwrap();
	}
}

impl Serializable for Bytes {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&CompactInteger::from(self.len()))
			.append_slice(&self);
	}
}

#[cfg(test)]
mod tests {
	use primitives::Bytes;
	use super::{Stream, serialize};

	#[test]
	fn test_stream_append() {
		let mut stream = Stream::default();

		stream
			.append(&1u8)
			.append(&2u16)
			.append(&3u32)
			.append(&4u64);

		let expected = vec![
			1u8,
			2, 0,
			3, 0, 0, 0,
			4, 0, 0, 0, 0, 0, 0, 0,
		];

		assert_eq!(expected, stream.out());
	}

	#[test]
	fn test_bytes_serialize() {
		let expected = vec![0x02, 0x01, 0x45];
		let bytes: Bytes = "0145".into();
		assert_eq!(expected, serialize(&bytes));
	}

}
