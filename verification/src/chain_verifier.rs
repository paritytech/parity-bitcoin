//! Bitcoin chain verifier

use std::sync::Arc;

use db::{self, BlockRef};
use chain;
use super::{Verify, VerificationResult, Chain, Error, TransactionError};
use utils;

const BLOCK_MAX_FUTURE: i64 = 2 * 60 * 60; // 2 hours

pub struct ChainVerifier {
	store: Arc<db::Store>,
}

impl ChainVerifier {
	pub fn new(store: Arc<db::Store>) -> Self {
		ChainVerifier { store: store }
	}

	fn verify_transaction(&self, transaction: &chain::Transaction) -> Result<(), TransactionError> {
		//use script::{TransactionInputSigner, TransactionSignatureChecker, VerificationFlags, verify_script};

		for (input_index, input) in transaction.inputs().iter().enumerate() {
			let parent_transaction = match self.store.transaction(&input.previous_output.hash) {
				Some(tx) => tx,
				None => { return Err(TransactionError::Input(input_index)); }
			};
			if parent_transaction.outputs.len() <= input.previous_output.index as usize {
				return Err(TransactionError::Input(input_index));
			}


			// signature verification

//			let signer: TransactionInputSigner = transaction.clone().into();
//			let paired_output = parent_transaction.outputs[input.previous_output.index as usize];
//			let checker = TransactionSignatureChecker {
//				signer: signer,
//				input_index: input_index,
//			};
//			let input: Script = input.script_sig().into();
//			let output: Script = paired_output.script_pubkey.into();
//			let flags = VerificationFlags::default().verify_p2sh(true);
//
//			if !verify_script(&input, &output, &flags, &checker).unwrap_or(|e| {
//					// todo: log error here
//					println!("transaction signature verification failure: {:?}", e);
//					false
//				})
//			{
//				return Err(TransactionError::Signature(input_index))
//			}
		}

		Ok(())
	}
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {
		let hash = block.hash();

		// There should be at least 1 transaction
		if block.transactions().is_empty() {
			return Err(Error::Empty);
		}

		// target difficulty threshold
		if !utils::check_nbits(&hash, block.header().nbits) {
			return Err(Error::Pow);
		}

		// check if block timestamp is not far in the future
		if utils::age(block.header().time) < -BLOCK_MAX_FUTURE {
			return Err(Error::Timestamp);
		}

		// check first transaction is a coinbase transaction
		if !block.transactions()[0].is_coinbase() {
			return Err(Error::Coinbase)
		}

		// verify transactions
		for (idx, transaction) in block.transactions().iter().skip(1).enumerate() {
			try!(self.verify_transaction(transaction).map_err(|e| Error::Transaction(idx, e)));
		}

		let _parent = match self.store.block(BlockRef::Hash(block.header().previous_header_hash.clone())) {
			Some(b) => b,
			None => { return Ok(Chain::Orphan); }
		};

		Ok(Chain::Main)
	}
}

#[cfg(test)]
mod tests {

	use super::ChainVerifier;
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
			for (idx, block) in blocks.iter().enumerate() {
				let hash = block.hash();
				storage.blocks.insert(hash.clone(), block.clone());
				storage.heights.insert(idx, hash);
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

		fn insert_block(&self, _block: &chain::Block) -> Result<(), db::Error> {
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

}
