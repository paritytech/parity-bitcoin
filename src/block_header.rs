
use stream::{Serializable, Stream};

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

#[cfg(test)]
mod tests {
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
}
