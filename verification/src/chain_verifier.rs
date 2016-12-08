//! Bitcoin chain verifier

use std::collections::BTreeSet;
use scoped_pool::Pool;
use hash::H256;
use db::{self, BlockLocation, PreviousTransactionOutputProvider, BlockHeaderProvider};
use network::{Magic, ConsensusParams};
use script::Script;
use error::{Error, TransactionError};
use {Verify, chain, utils};

const BLOCK_MAX_FUTURE: i64 = 2 * 60 * 60; // 2 hours
const COINBASE_MATURITY: u32 = 100; // 2 hours
const MAX_BLOCK_SIGOPS: usize = 20000;
const MAX_BLOCK_SIZE: usize = 1000000;

const TRANSACTIONS_VERIFY_THREADS: usize = 4;
const TRANSACTIONS_VERIFY_PARALLEL_THRESHOLD: usize = 16;

#[derive(PartialEq, Debug)]
/// Block verification chain
pub enum Chain {
	/// Main chain
	Main,
	/// Side chain
	Side,
	/// Orphan (no known parent)
	Orphan,
}

/// Verification result
pub type VerificationResult = Result<Chain, Error>;

pub struct ChainVerifier {
	store: db::SharedStore,
	skip_pow: bool,
	skip_sig: bool,
	network: Magic,
	consensus_params: ConsensusParams,
	pool: Pool,
}

impl ChainVerifier {
	pub fn new(store: db::SharedStore, network: Magic) -> Self {
		ChainVerifier {
			store: store,
			skip_pow: false,
			skip_sig: false,
			network: network,
			consensus_params: network.consensus_params(),
			pool: Pool::new(TRANSACTIONS_VERIFY_THREADS),
		}
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

	pub fn verify_p2sh(&self, time: u32) -> bool {
		time >= self.consensus_params.bip16_time
	}

	pub fn verify_clocktimeverify(&self, height: u32) -> bool {
		height >= self.consensus_params.bip65_height
	}

	/// Returns previous transaction output.
	/// NOTE: This function expects all previous blocks to be already in database.
	fn previous_transaction_output<T>(&self, prevout_provider: &T, prevout: &chain::OutPoint) -> Option<chain::TransactionOutput>
		where T: PreviousTransactionOutputProvider {
		self.store.transaction(&prevout.hash)
			.and_then(|tx| tx.outputs.into_iter().nth(prevout.index as usize))
			.or_else(|| prevout_provider.previous_transaction_output(prevout))
	}

	/// Returns number of transaction signature operations.
	/// NOTE: This function expects all previous blocks to be already in database.
	fn transaction_sigops(&self, block: &db::IndexedBlock, transaction: &chain::Transaction, bip16_active: bool) -> usize {
		let output_sigops: usize = transaction.outputs.iter().map(|output| {
			let output_script: Script = output.script_pubkey.clone().into();
			output_script.sigops_count(false)
		}).sum();

		if transaction.is_coinbase() {
			return output_sigops;
		}

		let input_sigops: usize = transaction.inputs.iter().map(|input| {
			let input_script: Script = input.script_sig.clone().into();
			let mut sigops = input_script.sigops_count(false);
			if bip16_active {
				let previous_output = self.previous_transaction_output(block, &input.previous_output)
					.expect("missing tx, out of order verification or malformed db");
				let prevout_script: Script = previous_output.script_pubkey.into();
				sigops += input_script.pay_to_script_hash_sigops(&prevout_script);
			}
			sigops
		}).sum();

		input_sigops + output_sigops
	}

	/// Returns number of block signature operations.
	/// NOTE: This function expects all previous blocks to be already in database.
	fn block_sigops(&self, block: &db::IndexedBlock) -> usize {
		// strict pay-to-script-hash signature operations count toward block
		// signature operations limit is enforced with BIP16
		let bip16_active = self.verify_p2sh(block.header().time);
		block.transactions().map(|(_, tx)| self.transaction_sigops(block, tx, bip16_active)).sum()
	}

	fn ordered_verify(&self, block: &db::IndexedBlock, at_height: u32) -> Result<(), Error> {
		if !block.is_final(at_height) {
			return Err(Error::NonFinalBlock);
		}

		// transaction verification including number of signature operations checking
		if self.block_sigops(block) > MAX_BLOCK_SIGOPS {
			return Err(Error::MaximumSigops);
		}

		let block_hash = block.hash();

		// check that difficulty matches the adjusted level
		//if let Some(work) = self.work_required(block, at_height) {
		if at_height != 0 && !self.skip_pow {
			let work = utils::work_required(
				block.header().previous_header_hash.clone(),
				block.header().time,
				at_height,
				self.store.as_block_header_provider(),
				self.network
			);
			if !self.skip_pow && work != block.header().bits {
				trace!(target: "verification", "pow verification error at height: {}", at_height);
				trace!(target: "verification", "expected work: {:?}, got {:?}", work, block.header().bits);
				return Err(Error::Difficulty);
			}
		}

		let coinbase_spends = block.transactions()
			.nth(0)
			.expect("block emptyness should be checked at this point")
			.1
			.total_spends();

		// bip30
		for (tx_index, (tx_hash, _)) in block.transactions().enumerate() {
			if let Some(meta) = self.store.transaction_meta(tx_hash) {
				if !meta.is_fully_spent() && !self.consensus_params.is_bip30_exception(&block_hash, at_height) {
					return Err(Error::Transaction(tx_index, TransactionError::UnspentTransactionWithTheSameHash));
				}
			}
		}

		let mut total_unspent = 0u64;
		for (tx_index, (_, tx)) in block.transactions().enumerate().skip(1) {
			let mut total_claimed: u64 = 0;
			for input in &tx.inputs {
				// Coinbase maturity check
				if let Some(previous_meta) = self.store.transaction_meta(&input.previous_output.hash) {
					// check if it exists only
					// it will fail a little later if there is no transaction at all
					if previous_meta.is_coinbase() &&
						(at_height < COINBASE_MATURITY || at_height - COINBASE_MATURITY < previous_meta.height()) {
						return Err(Error::Transaction(tx_index, TransactionError::Maturity));
					}
				}

				let previous_output = self.previous_transaction_output(block, &input.previous_output)
					.expect("missing tx, out of order verification or malformed db");

				total_claimed += previous_output.value;
			}

			let total_spends = tx.total_spends();

			if total_claimed < total_spends {
				return Err(Error::Transaction(tx_index, TransactionError::Overspend));
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

	pub fn verify_transaction<T>(
		&self,
		prevout_provider: &T,
		height: u32,
		time: u32,
		transaction: &chain::Transaction,
		sequence: usize
	) -> Result<(), TransactionError> where T: PreviousTransactionOutputProvider {

		use script::{
			TransactionInputSigner,
			TransactionSignatureChecker,
			VerificationFlags,
			Script,
			verify_script,
		};

		if sequence == 0 {
			return Ok(());
		}

		// must not be coinbase (sequence = 0 is returned above)
		if transaction.is_coinbase() { return Err(TransactionError::MisplacedCoinbase(sequence)); }

		for (input_index, input) in transaction.inputs().iter().enumerate() {
			// signature verification
			let signer: TransactionInputSigner = transaction.clone().into();
			let paired_output = match self.previous_transaction_output(prevout_provider, &input.previous_output) {
				Some(output) => output,
				_ => return Err(TransactionError::UnknownReference(input.previous_output.hash.clone()))
			};

			if prevout_provider.is_spent(&input.previous_output) {
				return Err(TransactionError::UsingSpentOutput(input.previous_output.hash.clone(), input.previous_output.index))
			}

			let checker = TransactionSignatureChecker {
				signer: signer,
				input_index: input_index,
			};
			let input: Script = input.script_sig.clone().into();
			let output: Script = paired_output.script_pubkey.into();

			let flags = VerificationFlags::default()
				.verify_p2sh(self.verify_p2sh(time))
				.verify_clocktimeverify(self.verify_clocktimeverify(height));

			// for tests only, skips as late as possible
			if self.skip_sig { continue; }

			if let Err(e) = verify_script(&input, &output, &flags, &checker) {
				trace!(target: "verification", "transaction signature verification failure: {:?}", e);
				trace!(target: "verification", "input:\n{}", input);
				trace!(target: "verification", "output:\n{}", output);
				// todo: log error here
				return Err(TransactionError::Signature(input_index))
			}
		}

		Ok(())
	}

	pub fn verify_block_header(
		&self,
		block_header_provider: &BlockHeaderProvider,
		hash: &H256,
		header: &chain::BlockHeader
	) -> Result<(), Error> {
		// target difficulty threshold
		if !self.skip_pow && !utils::is_valid_proof_of_work(self.network.max_bits(), header.bits, hash) {
			return Err(Error::Pow);
		}

		// check if block timestamp is not far in the future
		if utils::age(header.time) < -BLOCK_MAX_FUTURE {
			return Err(Error::FuturisticTimestamp);
		}

		if let Some(median_timestamp) = self.median_timestamp(block_header_provider, header) {
			// TODO: make timestamp validation on testnet work...
			if self.network != Magic::Testnet && median_timestamp >= header.time {
				trace!(
					target: "verification", "median timestamp verification failed, median: {}, current: {}",
					median_timestamp,
					header.time
				);
				return Err(Error::Timestamp);
			}
		}

		Ok(())
	}

	fn verify_block(&self, block: &db::IndexedBlock) -> VerificationResult {
		use task::Task;

		let hash = block.hash();

		// There should be at least 1 transaction
		if block.transaction_count() == 0 {
			return Err(Error::Empty);
		}

		// block header checks
		try!(self.verify_block_header(self.store.as_block_header_provider(), &hash, block.header()));

		// todo: serialized_size function is at least suboptimal
		let size = block.size();
		if size > MAX_BLOCK_SIZE {
			return Err(Error::Size(size))
		}

		// verify merkle root
		if block.merkle_root() != block.header().merkle_root_hash {
			return Err(Error::MerkleRoot);
		}

		let first_tx = block.transactions().nth(0).expect("transaction count is checked above to be greater than 0").1;
		// check first transaction is a coinbase transaction
		if !first_tx.is_coinbase() {
			return Err(Error::Coinbase)
		}
		// check that coinbase has a valid signature
		// is_coinbase() = true above guarantees that there is at least one input
		let coinbase_script_len = first_tx.inputs[0].script_sig.len();
		if coinbase_script_len < 2 || coinbase_script_len > 100 {
			return Err(Error::CoinbaseSignatureLength(coinbase_script_len));
		}

		let location = match self.store.accepted_location(block.header()) {
			Some(location) => location,
			None => return Ok(Chain::Orphan),
		};

		if block.transaction_count() > TRANSACTIONS_VERIFY_PARALLEL_THRESHOLD {
			// todo: might use on-stack vector (smallvec/elastic array)
			let mut transaction_tasks: Vec<Task> = Vec::with_capacity(TRANSACTIONS_VERIFY_THREADS);
			let mut last = 0;
			for num_task in 0..TRANSACTIONS_VERIFY_THREADS {
				let from = last;
				last = ::std::cmp::max(1, block.transaction_count() / TRANSACTIONS_VERIFY_THREADS);
				if num_task == TRANSACTIONS_VERIFY_THREADS - 1 { last = block.transaction_count(); };
				transaction_tasks.push(Task::new(block, location.height(), from, last));
			}

			self.pool.scoped(|scope| {
				for task in transaction_tasks.iter_mut() {
					scope.execute(move || task.progress(self))
				}
			});


			for task in transaction_tasks.into_iter() {
				if let Err((index, tx_err)) = task.result() {
					return Err(Error::Transaction(index, tx_err));
				}
			}
		}
		else {
			for (index, (_, tx)) in block.transactions().enumerate() {
				if let Err(tx_err) = self.verify_transaction(block, location.height(), block.header().time, tx, index) {
					return Err(Error::Transaction(index, tx_err));
				}
			}
		}

		// todo: pre-process projected block number once verification is parallel!
		match location {
			BlockLocation::Main(block_number) => {
				try!(self.ordered_verify(block, block_number));
				Ok(Chain::Main)
			},
			BlockLocation::Side(block_number) => {
				try!(self.ordered_verify(block, block_number));
				Ok(Chain::Side)
			},
		}
	}

	fn median_timestamp(&self, block_header_provider: &BlockHeaderProvider, header: &chain::BlockHeader) -> Option<u32> {
		let mut timestamps = BTreeSet::new();
		let mut block_ref = header.previous_header_hash.clone().into();
		// TODO: optimize it, so it does not make 11 redundant queries each time
		for _ in 0..11 {
			let previous_header = match block_header_provider.block_header(block_ref) {
				Some(h) => h,
				None => { break; }
			};
			timestamps.insert(previous_header.time);
			block_ref = previous_header.previous_header_hash.into();
		}

		if timestamps.len() > 2 {
			let timestamps: Vec<_> = timestamps.into_iter().collect();
			Some(timestamps[timestamps.len() / 2])
		}
		else { None }
	}
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &db::IndexedBlock) -> VerificationResult {
		let result = self.verify_block(block);
		trace!(
			target: "verification", "Block {} (transactions: {}) verification finished. Result {:?}",
			block.hash().to_reversed_str(),
			block.transaction_count(),
			result,
		);
		result
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use db::{TestStorage, Storage, Store, BlockStapler, IndexedBlock};
	use network::Magic;
	use devtools::RandomTempPath;
	use {script, test_data};
	use super::ChainVerifier;
	use super::super::{Verify, Chain, Error, TransactionError};

	#[test]
	fn verify_orphan() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b2 = test_data::block_h2();
		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet);

		assert_eq!(Chain::Orphan, verifier.verify(&b2.into()).unwrap());
	}

	#[test]
	fn verify_smoky() {
		let storage = TestStorage::with_blocks(&vec![test_data::genesis()]);
		let b1 = test_data::block_h1();
		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet);
		assert_eq!(Chain::Main, verifier.verify(&b1.into()).unwrap());
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
		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet);
		assert_eq!(Chain::Main, verifier.verify(&b1.into()).unwrap());
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

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Err(Error::Transaction(
			1,
			TransactionError::Maturity,
		));

		assert_eq!(expected, verifier.verify(&block.into()));
	}

	#[test]
	fn non_coinbase_happy() {
		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
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

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Ok(Chain::Main);
		assert_eq!(expected, verifier.verify(&block.into()));
	}


	#[test]
	fn transaction_references_same_block_happy() {
		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		storage.insert_block(&genesis).expect("Genesis should be inserted with no errors");
		let genesis_coinbase = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(30).build()
				.output().value(20).build()
				.build()
			.derived_transaction(1, 0)
				.output().value(30).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Ok(Chain::Main);
		assert_eq!(expected, verifier.verify(&block.into()));
	}

	#[test]
	fn transaction_references_same_block_overspend() {
		let path = RandomTempPath::create_dir();
		let storage = Storage::new(path.as_path()).unwrap();

		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		storage.insert_block(&genesis).expect("Genesis should be inserted with no errors");
		let genesis_coinbase = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(30).build()
				.output().value(20).build()
				.build()
			.derived_transaction(1, 0)
				.output().value(35).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Err(Error::Transaction(2, TransactionError::Overspend));
		assert_eq!(expected, verifier.verify(&block.into()));
	}

	#[test]
	#[ignore]
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

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Ok(Chain::Main);

		assert_eq!(expected, verifier.verify(&block.into()))
	}

	#[test]
	fn sigops_overflow_block() {
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

		let mut builder_tx1 = script::Builder::default();
		for _ in 0..11000 {
			builder_tx1 = builder_tx1.push_opcode(script::Opcode::OP_CHECKSIG)
		}

		let mut builder_tx2 = script::Builder::default();
		for _ in 0..11000 {
			builder_tx2 = builder_tx2.push_opcode(script::Opcode::OP_CHECKSIG)
		}

		let block: IndexedBlock = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input()
					.hash(reference_tx.clone())
					.signature_bytes(builder_tx1.into_script().to_bytes())
					.build()
				.build()
			.transaction()
				.input()
					.hash(reference_tx)
					.signature_bytes(builder_tx2.into_script().to_bytes())
					.build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build()
			.into();

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Err(Error::MaximumSigops);
		assert_eq!(expected, verifier.verify(&block.into()));
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

		let block: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(5000000001).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build()
			.into();

		let verifier = ChainVerifier::new(Arc::new(storage), Magic::Testnet).pow_skip().signatures_skip();

		let expected = Err(Error::CoinbaseOverspend {
			expected_max: 5000000000,
			actual: 5000000001
		});

		assert_eq!(expected, verifier.verify(&block.into()));
	}
}
