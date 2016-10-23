//! Bitcoin chain verifier

use std::sync::Arc;

use db::{self, BlockRef};
use chain;
use super::{Verify, VerificationResult, Chain, Error};
use primitives::hash::H256;
use byteorder::{LittleEndian, BigEndian, ByteOrder};

pub struct ChainVerifier {
	store: Arc<db::Store>,
}

impl ChainVerifier {
	pub fn new(store: Arc<db::Store>) -> Self {
		ChainVerifier { store: store }
	}
}

fn check_nbits(hash: &H256, n_bits: u32) -> bool {
	let hash_bytes: &[u8] = &**hash;

	let mut nb = [0u8; 4];
	BigEndian::write_u32(&mut nb, n_bits);
	let shift = (nb[0] - 3) as usize; // total shift for mantissa

	if shift >= 30 { return false; } // invalid shift

	let should_be_zero = shift + 3..32;
	let should_be_le = shift..shift + 3;

	for z_check in should_be_zero {
		if hash_bytes[z_check as usize] != 0 { return false; }
	}

	// making u32 from 3 bytes
	let mut order = 0;
	let hash_val: u32 = hash_bytes[should_be_le].iter().fold(0u32, |s, a| { let r = s + ((*a as u32) << order); order = order + 8; r });

	// using 3 bytes leftover of nbits
	nb[0] = 0;
	let threshold = BigEndian::read_u32(&nb);
	if hash_val < threshold {
		return true;
	}
	else if hash_val > threshold {
		return false;
	}

	// the case when hash effective bits are equal to nbits
	// then the rest of the hash must be zero
	for byte in hash_bytes[0..shift].iter() { if *byte != 0 { return false; } }

	return true;
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {
		let hash = block.hash();

		// There should be at least 1 transaction
		if block.transactions().is_empty() {
			return Err(Error::Empty);
		}

		// target difficulty threshold
		if !check_nbits(&hash, block.header().nbits) {
			return Err(Error::Pow);
		}

		let parent = match self.store.block(BlockRef::Hash(block.header().previous_header_hash.clone())) {
			Some(b) => b,
			None => { return Ok(Chain::Orphan); }
		};

		Ok(Chain::Main)
	}
}

#[cfg(test)]
mod tests {

	use super::{check_nbits, ChainVerifier};
	use super::super::{Verify, Chain};
	use primitives::hash::H256;
	use chain;
	use std::collections::HashMap;
	use db::{self, BlockRef, Store};
	use serialization;
	use serialization::bytes::Bytes;
	use test_data;
	use std::sync::Arc;

	#[derive(Default)]
	struct TestStorage {
		blocks: HashMap<H256, chain::Block>,
		heights: HashMap<usize, H256>,
	}

	impl TestStorage {
		fn resolve_hash(&self, block_ref: BlockRef) -> Option<H256> {
			match block_ref {
				BlockRef::Number(n) => self.block_hash(n),
				BlockRef::Hash(h) => Some(h),
			}
		}

		fn with_blocks(blocks: &[chain::Block]) -> Self {
			let mut storage = TestStorage::default();
			let mut height = 0;
			for (idx, block) in blocks.iter().enumerate() {
				let hash = block.hash();
				storage.blocks.insert(hash.clone(), block.clone());
				storage.heights.insert(height, hash);
			}
			storage
		}
	}

	impl Store for TestStorage {
		fn block_hash(&self, number: u64) -> Option<H256> {
			self.heights.get(&(number as usize)).cloned()
		}

		fn block_header_bytes(&self, block_ref: BlockRef) -> Option<Bytes> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.map(|ref block| serialization::serialize(block.header()))
		}

		fn block_transaction_hashes(&self, block_ref: BlockRef) -> Vec<H256> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.map(|ref block| block.transactions().iter().map(|tx| tx.hash()).collect())
				.unwrap_or(Vec::new())
		}

		fn transaction_bytes(&self, hash: &H256) -> Option<Bytes> {
			self.transaction(hash).map(|tx| serialization::serialize(&tx))
		}

		fn transaction(&self, hash: &H256) -> Option<chain::Transaction> {
			self.blocks.iter().flat_map(|(_, b)| b.transactions())
				.find(|ref tx| tx.hash() == *hash)
				.cloned()
		}

		fn block_transactions(&self, block_ref: BlockRef) -> Vec<chain::Transaction> {
			self.block(block_ref)
				.map(|b| b.transactions().iter().cloned().collect())
				.unwrap_or(Vec::new())
		}

		fn block(&self, block_ref: BlockRef) -> Option<chain::Block> {
			self.resolve_hash(block_ref)
				.and_then(|ref h| self.blocks.get(h))
				.cloned()
		}

		fn insert_block(&self, block: &chain::Block) -> Result<(), db::Error> {
			Ok(())
		}
	}

	#[test]
	fn verify_orphan() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b2 = test_data::block_h2();
		let verifier = ChainVerifier::new(Arc::new(storage));

		assert_eq!(Chain::Orphan, verifier.verify(&b2).unwrap());
	}

	#[test]
	fn verify_smoky() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b1 = test_data::block_h1();
		let verifier = ChainVerifier::new(Arc::new(storage));
		assert_eq!(Chain::Main, verifier.verify(&b1).unwrap());
	}

	#[test]
	fn nbits() {
		// strictly equal
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000000");
		let nbits = 0x181bc330u32;
		assert!(check_nbits(&hash, nbits));

		// nbits match but not equal (greater)
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000001");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(&hash, nbits));

		// greater
		let hash = H256::from_reversed_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(&hash, nbits));


		// some real examples
		let hash = H256::from_reversed_str("000000000000000001f942eb4bfa0aeccb6a14c268f4c72d5fff17270da771b9");
		let nbits = 404129525;
		assert!(check_nbits(&hash, nbits));

		// some real examples
		let hash = H256::from_reversed_str("00000000000000000e753ef636075711efd2cbf5a8473c7c5b67755a3701e0c2");
		let nbits = 404129525;
		assert!(check_nbits(&hash, nbits));
	}
}
