use std::fmt;
use ser::{
	Deserializable, Reader, Error as ReaderError,
	Serializable, Stream
};
use hash::{H256, h256_to_str};

#[derive(Debug, PartialEq)]
pub struct BlockHeader {
	version: u32,
	previous_header_hash: H256,
	merkle_root_hash: H256,
	time: u32,
	nbits: u32,
	nonce: u32,
}

impl fmt::Display for BlockHeader {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		try!(writeln!(f, "version: {}", self.version));;
		try!(writeln!(f, "previous header hash: {}", h256_to_str(&self.previous_header_hash)));
		try!(writeln!(f, "merkle root hash: {}", h256_to_str(&self.merkle_root_hash)));
		try!(writeln!(f, "time: {}", self.time));
		try!(writeln!(f, "nbits: {}", self.nbits));
		writeln!(f, "nonce: {}", self.nonce)
	}
}

impl Serializable for BlockHeader {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.version)
			.append(&self.previous_header_hash)
			.append(&self.merkle_root_hash)
			.append(&self.time)
			.append(&self.nbits)
			.append(&self.nonce);
	}
}

impl Deserializable for BlockHeader {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let version = try!(reader.read());
		let previous_header_hash = try!(reader.read());
		let merkle_root_hash = try!(reader.read());
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
	use ser::{Reader, Error as ReaderError, Stream};
	use super::BlockHeader;

	#[test]
	fn test_block_header_stream() {
		let block_header = BlockHeader {
			version: 1,
			previous_header_hash: [2; 32].into(),
			merkle_root_hash: [3; 32].into(),
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
			previous_header_hash: [2; 32].into(),
			merkle_root_hash: [3; 32].into(),
			time: 4,
			nbits: 5,
			nonce: 6,
		};

		assert_eq!(expected, reader.read().unwrap());
		assert_eq!(ReaderError::UnexpectedEnd, reader.read::<BlockHeader>().unwrap_err());
	}
}
