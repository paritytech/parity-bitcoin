use bytes::Bytes;
use hash::{H96, H160, H256, H264, H512, H520};
use compact_integer::CompactInteger;
use {Serializable, Stream, Deserializable, Reader, Error};

macro_rules! impl_ser_for_hash {
	($name: ident, $size: expr) => {
		impl Serializable for $name {
			fn serialize(&self, stream: &mut Stream) {
				stream.append_slice(&**self);
			}
		}

		impl Deserializable for $name {
			fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
				let slice = try!(reader.read_slice($size));
				let mut result = Self::default();
				result.copy_from_slice(slice);
				Ok(result)
			}
		}
	}
}

impl_ser_for_hash!(H96, 12);
impl_ser_for_hash!(H160, 20);
impl_ser_for_hash!(H256, 32);
impl_ser_for_hash!(H264, 33);
impl_ser_for_hash!(H512, 64);
impl_ser_for_hash!(H520, 65);

impl Serializable for Bytes {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&CompactInteger::from(self.len()))
			.append_slice(&self);
	}
}

impl Deserializable for Bytes {
	fn deserialize(reader: &mut Reader) -> Result<Self, Error> where Self: Sized {
		let len = try!(reader.read::<CompactInteger>());
		reader.read_slice(len.into()).map(|b| b.to_vec().into())
	}
}

#[cfg(test)]
mod tests {
	use bytes::Bytes;
	use {serialize, deserialize};

	#[test]
	fn test_bytes_deserialize() {
		let raw: Bytes = "020145".into();
		let expected: Bytes = "0145".into();
		assert_eq!(expected, deserialize(&raw).unwrap());
	}

	#[test]
	fn test_bytes_serialize() {
		let expected: Bytes = "020145".into();
		let bytes: Bytes = "0145".into();
		assert_eq!(expected, serialize(&bytes));
	}
}
