//! Bitcoin chain verifier

use std::sync::Arc;

use db::{self, BlockRef, BlockLocation};
use chain::{self, RepresentH256};
use super::{Verify, VerificationResult, Chain, Error, TransactionError, ContinueVerify};
use utils;

const BLOCK_MAX_FUTURE: i64 = 2 * 60 * 60; // 2 hours
const COINBASE_MATURITY: u32 = 100; // 2 hours

pub struct ChainVerifier {
	store: Arc<db::Store>,
	skip_pow: bool,
	skip_sig: bool,
}

impl ChainVerifier {
	pub fn new(store: Arc<db::Store>) -> Self {
		ChainVerifier { store: store, skip_pow: false, skip_sig: false }
	}

	#[cfg(test)]
	pub fn pow_skip(mut self) -> Self {
		self.skip_pow = true;
		self
	}

	#[cfg(test)]
	pub fn signatures_skip(mut self) -> Self {
		self.skip_sig = true;
		self
	}

	fn ordered_verify(&self, block: &chain::Block, at_height: u32) -> Result<(), Error> {

		let coinbase_spends = block.transactions()[0].total_spends();

		let mut total_unspent = 0u64;
		for (tx_index, tx) in block.transactions().iter().skip(1).enumerate() {

			let mut total_claimed: u64 = 0;

			for (_, input) in tx.inputs.iter().enumerate() {

				// Coinbase maturity check
				let previous_meta = try!(
					self.store
						.transaction_meta(&input.previous_output.hash)
						.ok_or(
							Error::Transaction(tx_index, TransactionError::UnknownReference(input.previous_output.hash.clone()))
						)
				);

				if previous_meta.is_coinbase()
					&& (at_height < COINBASE_MATURITY ||
						at_height - COINBASE_MATURITY < previous_meta.height())
				{
					return Err(Error::Transaction(tx_index+1, TransactionError::Maturity));
				}

				let reference_tx = try!(
					self.store.transaction(&input.previous_output.hash)
						.ok_or(
							Error::Transaction(tx_index+1, TransactionError::UnknownReference(input.previous_output.hash.clone()))
						)
				);

				let output = try!(reference_tx.outputs.get(input.previous_output.index as usize)
					.ok_or(
						Error::Transaction(tx_index+1, TransactionError::Input(input.previous_output.index as usize))
					)
				);

				total_claimed += output.value;
			}

			let total_spends = tx.total_spends();

			if total_claimed < total_spends {
				return Err(Error::Transaction(tx_index+1, TransactionError::Overspend));
			}

			// total_claimed is greater than total_spends, checked above and returned otherwise, cannot overflow; qed
			total_unspent += total_claimed - total_spends;
		}

		let expected_max = utils::block_reward_satoshi(at_height) + total_unspent;
		if coinbase_spends > expected_max{
			return Err(Error::CoinbaseOverspend { expected_max: expected_max, actual: coinbase_spends });
		}

		Ok(())
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
						Some(tx) => tx,
						None => { return Err(TransactionError::Inconclusive(input.previous_output.hash.clone())); },
					}
				},
			};
			if parent_transaction.outputs.len() <= input.previous_output.index as usize {
				return Err(TransactionError::Input(input_index));
			}

			if self.skip_sig { continue; }
			// signature verification
			let signer: TransactionInputSigner = transaction.clone().into();
			let paired_output = &parent_transaction.outputs[input.previous_output.index as usize];
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
		if !self.skip_pow && !utils::check_nbits(&hash, block.header().nbits) {
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
			try!(self.verify_transaction(block, transaction).map_err(|e| Error::Transaction(idx+1, e)));
		}

		// todo: pre-process projected block number once verification is parallel!
		match self.store.accepted_location(block.header()) {
			None => {
				Ok(Chain::Orphan)
			},
			Some(BlockLocation::Main(block_number)) => {
				try!(self.ordered_verify(block, block_number));
				Ok(Chain::Main)
			},
			Some(BlockLocation::Side(block_number)) => {
				try!(self.ordered_verify(block, block_number));
				Ok(Chain::Side)
			},
		}
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
	use db::{TestStorage, Storage, Store};
	use test_data;
	use std::sync::Arc;
	use devtools::RandomTempPath;
	use chain::RepresentH256;

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
			1,
			TransactionError::Inconclusive("c997a5e56e104102fa209c6a852dd90660a20b2d9c352423edce25857fcd3704".into())
		));
		assert_eq!(should_be, verifier.verify(&b170));
	}

	#[test]
	fn coinbase_maturity() {

		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		storage.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage)).pow_skip().signatures_skip();

		let expected = Err(Error::Transaction(
			1,
			TransactionError::Maturity,
		));

		assert_eq!(expected, verifier.verify(&block));
	}

	#[test]
	fn non_coinbase_happy() {
		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.build()
			.transaction()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		storage.insert_block(&genesis).unwrap();
		let reference_tx = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(reference_tx).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage)).pow_skip().signatures_skip();

		let expected = Ok(Chain::Main);
		assert_eq!(expected, verifier.verify(&block));
	}

	#[test]
	fn coinbase_happy() {

		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		storage.insert_block(&genesis).unwrap();
		let genesis_coinbase = genesis.transactions()[0].hash();

		// waiting 100 blocks for genesis coinbase to become valid
		for _ in 0..100 {
			storage.insert_block(
				&test_data::block_builder()
					.transaction().coinbase().build()
				.merkled_header().parent(genesis.hash()).build()
				.build()
			).expect("All dummy blocks should be inserted");
		}

		let best_hash = storage.best_block().expect("Store should have hash after all we pushed there").hash;

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.build()
			.merkled_header().parent(best_hash).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage)).pow_skip().signatures_skip();

		let expected = Ok(Chain::Main);

		assert_eq!(expected, verifier.verify(&block))
	}

	#[test]
	fn coinbase_overspend() {

		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction().coinbase().build()
			.merkled_header().build()
			.build();
		storage.insert_block(&genesis).unwrap();

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(5000000001).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage)).pow_skip().signatures_skip();

		let expected = Err(Error::CoinbaseOverspend {
			expected_max: 5000000000,
			actual: 5000000001
		});

		assert_eq!(expected, verifier.verify(&block));
	}
}
