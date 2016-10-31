//! Bitcoin chain verifier

use std::sync::Arc;

use db::{self, BlockRef};
use chain::{self, RepresentH256};
use super::{Verify, VerificationResult, Chain, Error, TransactionError, ContinueVerify};
use utils;

const BLOCK_MAX_FUTURE: i64 = 2 * 60 * 60; // 2 hours

pub struct ChainVerifier {
	store: Arc<db::Store>,
}

impl ChainVerifier {
	pub fn new(store: Arc<db::Store>) -> Self {
		ChainVerifier { store: store }
	}

	fn verify_transaction(&self, block: &chain::Block, transaction: &chain::Transaction) -> Result<(), TransactionError> {
		use script::{
			TransactionInputSigner,
			TransactionSignatureChecker,
			VerificationFlags,
			Script,
			verify_script,
		};

		for (input_index, input) in transaction.inputs().iter().enumerate() {
			let store_parent_transaction = self.store.transaction(&input.previous_output.hash);
			let parent_transaction = match store_parent_transaction {
				Some(ref tx) => tx,
				None => {
					match block.transactions.iter().filter(|t| t.hash() == input.previous_output.hash).nth(0) {
						Some(ref tx) => tx,
						None => { return Err(TransactionError::Inconclusive(input.previous_output.hash.clone())); },
					}
				},
			};
			if parent_transaction.outputs.len() <= input.previous_output.index as usize {
				return Err(TransactionError::Input(input_index));
			}

			// signature verification
			let signer: TransactionInputSigner = transaction.clone().into();
			let ref paired_output = parent_transaction.outputs[input.previous_output.index as usize];
			let checker = TransactionSignatureChecker {
				signer: signer,
				input_index: input_index,
			};
			let input: Script = input.script_sig().to_vec().into();
			let output: Script = paired_output.script_pubkey.to_vec().into();
			let flags = VerificationFlags::default().verify_p2sh(true);

			if let Err(e) =  verify_script(&input, &output, &flags, &checker) {
				println!("transaction signature verification failure: {:?}", e);
				// todo: log error here
				return Err(TransactionError::Signature(input_index))
			}
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

		// verify merkle root
		if block.merkle_root() != block.header().merkle_root_hash {
			return Err(Error::MerkleRoot);
		}

		// check first transaction is a coinbase transaction
		if !block.transactions()[0].is_coinbase() {
			return Err(Error::Coinbase)
		}

		// verify transactions (except coinbase)
		for (idx, transaction) in block.transactions().iter().skip(1).enumerate() {
			try!(self.verify_transaction(block, transaction).map_err(|e| Error::Transaction(idx, e)));
		}

		let _parent = match self.store.block(BlockRef::Hash(block.header().previous_header_hash.clone())) {
			Some(b) => b,
			None => { return Ok(Chain::Orphan); }
		};

		Ok(Chain::Main)
	}
}

impl ContinueVerify for ChainVerifier {
	type State = usize;

	fn continue_verify(&self, block: &chain::Block, state: usize) -> VerificationResult {
		// verify transactions (except coinbase)
		for (idx, transaction) in block.transactions().iter().skip(state-1).enumerate() {
			try!(self.verify_transaction(block, transaction).map_err(|e| Error::Transaction(idx, e)));
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
	use super::super::{Verify, Chain, Error, TransactionError};
	use db::TestStorage;
	use test_data;
	use std::sync::Arc;

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
	fn firtst_tx() {
		let storage = TestStorage::with_blocks(
			&vec![
				test_data::block_h9(),
				test_data::block_h169(),
			]
		);
		let b1 = test_data::block_h170();
		let verifier = ChainVerifier::new(Arc::new(storage));
		assert_eq!(Chain::Main, verifier.verify(&b1).unwrap());
	}

	#[test]
	fn unknown_transaction_returns_inconclusive() {
		let storage = TestStorage::with_blocks(
			&vec![
				test_data::block_h169(),
			]
		);
		let b170 = test_data::block_h170();
		let verifier = ChainVerifier::new(Arc::new(storage));

		let should_be = Err(Error::Transaction(
			0,
			TransactionError::Inconclusive("c997a5e56e104102fa209c6a852dd90660a20b2d9c352423edce25857fcd3704".into())
		));
		assert_eq!(should_be, verifier.verify(&b170));
	}

}
