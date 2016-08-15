use reader::{Deserializable, Reader, Error as ReaderError};
use stream::{Serializable, Stream};

#[derive(Debug, PartialEq)]
pub struct BlockHeader {
	version: u32,
	previous_header_hash: [u8; 32],
	merkle_root_hash: [u8; 32],
	time: u32,
	nbits: u32,
	nonce: u32,
}

impl Serializable for BlockHeader {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append_bytes(&self.previous_header_hash)
			.append_bytes(&self.merkle_root_hash)
			.append(&self.time)
			.append(&self.nbits)
			.append(&self.nonce);
	}
}

impl Deserializable for BlockHeader {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let version = try!(reader.read());
		let mut previous_header_hash = [0u8; 32];
		previous_header_hash.copy_from_slice(try!(reader.read_bytes(32)));
		let mut merkle_root_hash = [0u8; 32]; 
		merkle_root_hash.copy_from_slice(try!(reader.read_bytes(32)));
		let time = try!(reader.read());
		let nbits = try!(reader.read());
		let nonce = try!(reader.read());

		let block_header = BlockHeader {
			version: version,
			previous_header_hash: previous_header_hash,
			merkle_root_hash: merkle_root_hash,
			time: time,
			nbits: nbits,
			nonce: nonce,
		};

		Ok(block_header)
	}
}

#[cfg(test)]
mod tests {
	use reader::{Reader, Error as ReaderError};
	use stream::Stream;
	use super::BlockHeader;

	#[test]
	fn test_block_header_stream() {
		let block_header = BlockHeader {
			version: 1,
			previous_header_hash: [2; 32],
			merkle_root_hash: [3; 32],
			time: 4,
			nbits: 5,
			nonce: 6,
		};

		let mut stream = Stream::default();
		stream.append(&block_header);

		let expected = vec![
			1, 0, 0, 0,
			2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
			3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
			4, 0, 0, 0,
			5, 0, 0, 0,
			6, 0, 0, 0,
		];

		assert_eq!(stream.out(), expected);
	}

	#[test]
	fn test_block_header_reader() {
		let buffer = vec![
			1, 0, 0, 0,
			2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2, 2,
			3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3,
			4, 0, 0, 0,
			5, 0, 0, 0,
			6, 0, 0, 0,
		];

		let mut reader = Reader::new(&buffer);

		let expected = BlockHeader {
			version: 1,
			previous_header_hash: [2; 32],
			merkle_root_hash: [3; 32],
			time: 4,
			nbits: 5,
			nonce: 6,
		};

		assert_eq!(expected, reader.read().unwrap());
		assert_eq!(ReaderError::UnexpectedEnd, reader.read::<BlockHeader>().unwrap_err());
	}
}
