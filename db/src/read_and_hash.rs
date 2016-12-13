use std::io;
use crypto::{DHash256, Digest};
use primitives::hash::H256;
use serialization::{Reader, Error as ReaderError, Deserializable};

pub struct HashedData<T> {
	pub len: usize,
	pub hash: H256,
	pub data: T,
}

pub trait ReadAndHash {
	fn read_and_hash<T>(&mut self) -> Result<HashedData<T>, ReaderError> where T: Deserializable;
}

impl<R> ReadAndHash for Reader<R> where R: io::Read {
	fn read_and_hash<T>(&mut self) -> Result<HashedData<T>, ReaderError> where T: Deserializable {
		let mut len = 0usize;
		let mut hasher = DHash256::new();
		let data = self.read_with_proxy(|bytes| {
			len += bytes.len();
			hasher.input(bytes);
		})?;

		let result = HashedData {
			hash: hasher.finish(),
			data: data,
			len: len,
		};

		Ok(result)
	}
}
