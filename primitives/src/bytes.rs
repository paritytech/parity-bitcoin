//! Wrapper around `Vec<u8>`

use std::{ops, str, fmt, io, marker};
use hex::{ToHex, FromHex, FromHexError};
use heapsize::HeapSizeOf;

/// Wrapper around `Vec<u8>`
#[derive(Default, PartialEq, Clone, Eq, Hash)]
pub struct Bytes(Vec<u8>);

impl Bytes {
	pub fn new() -> Self {
		Bytes::default()
	}

	pub fn new_with_len(len: usize) -> Self {
		Bytes(vec![0; len])
	}

	pub fn take(self) -> Vec<u8> {
		self.0
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn append(&mut self, other: &mut Bytes) {
		self.0.append(&mut other.0);
	}

	pub fn split_off(&mut self, at: usize) -> Bytes {
		Bytes(self.0.split_off(at))
	}
}

impl HeapSizeOf for Bytes {
	fn heap_size_of_children(&self) -> usize {
		self.0.heap_size_of_children()
	}
}

impl<'a> From<&'a [u8]> for Bytes {
	fn from(v: &[u8]) -> Self {
		Bytes(v.into())
	}
}

impl From<Vec<u8>> for Bytes {
	fn from(v: Vec<u8>) -> Self {
		Bytes(v)
	}
}

impl From<Bytes> for Vec<u8> {
	fn from(bytes: Bytes) -> Self {
		bytes.0
	}
}

impl From<&'static str> for Bytes {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl str::FromStr for Bytes {
	type Err = FromHexError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		s.from_hex().map(Bytes)
	}
}

impl io::Write for Bytes {
	fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
		self.0.write(buf)
	}

	fn flush(&mut self) -> Result<(), io::Error> {
		self.0.flush()
	}
}

impl fmt::Debug for Bytes {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex::<String>())
	}
}

impl ops::Deref for Bytes {
	type Target = Vec<u8>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl ops::DerefMut for Bytes {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}

impl AsRef<[u8]> for Bytes {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl AsMut<[u8]> for Bytes {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0
	}
}

/// Wrapper around `Vec<u8>` which represent associated type
#[derive(Default, PartialEq, Clone)]
pub struct TaggedBytes<T> {
	bytes: Bytes,
	label: marker::PhantomData<T>,
}

impl<T> TaggedBytes<T> {
	pub fn new(bytes: Bytes) -> Self {
		TaggedBytes {
			bytes: bytes,
			label: marker::PhantomData,
		}
	}

	pub fn into_raw(self) -> Bytes {
		self.bytes
	}
}

impl<T> ops::Deref for TaggedBytes<T> {
	type Target = Vec<u8>;

	fn deref(&self) -> &Self::Target {
		&self.bytes.0
	}
}

impl<T> ops::DerefMut for TaggedBytes<T> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.bytes.0
	}
}

impl<T> AsRef<[u8]> for TaggedBytes<T> {
	fn as_ref(&self) -> &[u8] {
		&self.bytes.0
	}
}

impl<T> AsMut<[u8]> for TaggedBytes<T> {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.bytes.0
	}
}

#[cfg(test)]
mod tests {
	use super::Bytes;

	#[test]
	fn test_bytes_from_hex() {
		let bytes: Bytes = "0145".into();
		assert_eq!(bytes, vec![0x01, 0x45].into());
	}

	#[test]
	fn test_bytes_debug_formatter() {
		let bytes: Bytes = "0145".into();
		assert_eq!(format!("{:?}", bytes), "0145".to_owned());
	}
}
