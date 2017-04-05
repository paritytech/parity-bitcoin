use std::io;
use hex::FromHex;
use hash::H256;
use ser::{
	Deserializable, Reader, Error as ReaderError, deserialize,
	Serializable, Stream
};
use merkle_root::merkle_root;
use {BlockHeader, Transaction};
use super::RepresentH256;

#[derive(Debug, PartialEq, Clone)]
pub struct Block {
	pub block_header: BlockHeader,
	pub transactions: Vec<Transaction>,
}

impl Serializable for Block {
	fn serialize(&self, stream: &mut Stream) {
		stream
			.append(&self.block_header)
			.append_list(&self.transactions);
	}
}

impl Deserializable for Block {
	fn deserialize<T>(reader: &mut Reader<T>) -> Result<Self, ReaderError> where T: io::Read {
		let result = Block {
			block_header: try!(reader.read()),
			transactions: try!(reader.read_list()),
		};

		Ok(result)
	}
}

impl From<&'static str> for Block {
	fn from(s: &'static str) -> Self {
		deserialize(&s.from_hex().unwrap() as &[u8]).unwrap()
	}
}

impl RepresentH256 for Block {
	fn h256(&self) -> H256 { self.hash() }
}

impl Block {
	pub fn new(header: BlockHeader, transactions: Vec<Transaction>) -> Self {
		Block { block_header: header, transactions: transactions }
	}

	/// Returns block's merkle root.
	pub fn merkle_root(&self) -> H256 {
		let hashes = self.transactions.iter().map(Transaction::hash).collect::<Vec<H256>>();
		merkle_root(&hashes)
	}

	pub fn transactions(&self) -> &[Transaction] {
		&self.transactions
	}

	pub fn header(&self) -> &BlockHeader {
		&self.block_header
	}

	pub fn hash(&self) -> H256 {
		self.block_header.hash()
	}

	pub fn is_final(&self, height: u32) -> bool {
		self.transactions.iter().all(|t| t.is_final_in_block(height, self.block_header.time))
	}
}

#[cfg(test)]
mod tests {
	use hash::H256;
	use super::Block;

	// Block 80000
	// https://blockchain.info/rawblock/000000000043a8c0fd1d6f726790caa2a406010d19efd2780db27bdbbd93baf6
	// https://blockchain.info/rawblock/000000000043a8c0fd1d6f726790caa2a406010d19efd2780db27bdbbd93baf6?format=hex
	#[test]
	fn test_block_merkle_root_and_hash() {
		let block: Block = "01000000ba8b9cda965dd8e536670f9ddec10e53aab14b20bacad27b9137190000000000190760b278fe7b8565fda3b968b918d5fd997f993b23674c0af3b6fde300b38f33a5914ce6ed5b1b01e32f570201000000010000000000000000000000000000000000000000000000000000000000000000ffffffff0704e6ed5b1b014effffffff0100f2052a01000000434104b68a50eaa0287eff855189f949c1c6e5f58b37c88231373d8a59809cbae83059cc6469d65c665ccfd1cfeb75c6e8e19413bba7fbff9bc762419a76d87b16086eac000000000100000001a6b97044d03da79c005b20ea9c0e1a6d9dc12d9f7b91a5911c9030a439eed8f5000000004948304502206e21798a42fae0e854281abd38bacd1aeed3ee3738d9e1446618c4571d1090db022100e2ac980643b0b82c0e88ffdfec6b64e3e6ba35e7ba5fdd7d5d6cc8d25c6b241501ffffffff0100f2052a010000001976a914404371705fa9bd789a2fcd52d2c580b65d35549d88ac00000000".into();
		let merkle_root = H256::from_reversed_str("8fb300e3fdb6f30a4c67233b997f99fdd518b968b9a3fd65857bfe78b2600719");
		let hash = H256::from_reversed_str("000000000043a8c0fd1d6f726790caa2a406010d19efd2780db27bdbbd93baf6");
		assert_eq!(block.merkle_root(), merkle_root);
		assert_eq!(block.hash(), hash);
	}
}
