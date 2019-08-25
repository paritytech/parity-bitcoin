//! Bitcoin chain verifier

use hash::H256;
use chain::{IndexedBlock, IndexedBlockHeader, BlockHeader, IndexedTransaction};
use storage::{SharedStore, TransactionOutputProvider, BlockHeaderProvider, BlockOrigin,
	DuplexTransactionOutputProvider, NoopStore, CachedTransactionOutputProvider};
use network::ConsensusParams;
use error::{Error, TransactionError};
use canon::{CanonBlock, CanonTransaction};
use verify_chain::ChainVerifier;
use verify_header::HeaderVerifier;
use verify_transaction::MemoryPoolTransactionVerifier;
use accept_chain::ChainAcceptor;
use accept_transaction::MemoryPoolTransactionAcceptor;
use deployments::{Deployments, BlockDeployments};
use timestamp::median_timestamp_inclusive;
use {Verify, VerificationLevel};

pub struct BackwardsCompatibleChainVerifier {
	store: SharedStore,
	consensus: ConsensusParams,
	deployments: Deployments,
}

impl BackwardsCompatibleChainVerifier {
	pub fn new(store: SharedStore, consensus: ConsensusParams) -> Self {
		BackwardsCompatibleChainVerifier {
			store: store,
			consensus: consensus,
			deployments: Deployments::new(),
		}
	}

	fn verify_block(&self, verification_level: VerificationLevel, block: &IndexedBlock) -> Result<(), Error> {
		if verification_level == VerificationLevel::NoVerification {
			return Ok(());
		}

		let current_time = ::time::get_time().sec as u32;
		// first run pre-verification
		let chain_verifier = ChainVerifier::new(block, self.consensus.network, current_time);
		chain_verifier.check()?;

		assert_eq!(Some(self.store.best_block().hash), self.store.block_hash(self.store.best_block().number));
		let block_origin = self.store.block_origin(&block.header)?;
		trace!(
			target: "verification",
			"verify_block: {:?} best_block: {:?} block_origin: {:?}",
			block.hash().reversed(),
			self.store.best_block(),
			block_origin,
		);

		let median_time_past = median_timestamp_inclusive(block.header.raw.previous_header_hash.clone(), self.store.as_block_header_provider());
		let canon_block = CanonBlock::new(block);
		match block_origin {
			BlockOrigin::KnownBlock => {
				// there should be no known blocks at this point
				unreachable!();
			},
			BlockOrigin::CanonChain { block_number } => {
				let tx_out_provider = CachedTransactionOutputProvider::new(self.store.as_store().as_transaction_output_provider());
				let tx_meta_provider = self.store.as_store().as_transaction_meta_provider();
				let header_provider = self.store.as_store().as_block_header_provider();
				let deployments = BlockDeployments::new(&self.deployments, block_number, header_provider, &self.consensus);
				let chain_acceptor = ChainAcceptor::new(
					&tx_out_provider,
					tx_meta_provider,
					header_provider,
					&self.consensus,
					verification_level,
					canon_block,
					block_number,
					median_time_past,
					&deployments,
				);
				chain_acceptor.check()?;
			},
			BlockOrigin::SideChain(origin) => {
				let block_number = origin.block_number;
				let fork = self.store.fork(origin)?;
				let tx_out_provider = CachedTransactionOutputProvider::new(fork.store().as_transaction_output_provider());
				let tx_meta_provider = fork.store().as_transaction_meta_provider();
				let header_provider = fork.store().as_block_header_provider();
				let deployments = BlockDeployments::new(&self.deployments, block_number, header_provider, &self.consensus);
				let chain_acceptor = ChainAcceptor::new(
					&tx_out_provider,
					tx_meta_provider,
					header_provider,
					&self.consensus,
					verification_level,
					canon_block,
					block_number,
					median_time_past,
					&deployments,
				);
				chain_acceptor.check()?;
			},
			BlockOrigin::SideChainBecomingCanonChain(origin) => {
				let block_number = origin.block_number;
				let fork = self.store.fork(origin)?;
				let tx_out_provider = CachedTransactionOutputProvider::new(fork.store().as_transaction_output_provider());
				let tx_meta_provider = fork.store().as_transaction_meta_provider();
				let header_provider = fork.store().as_block_header_provider();
				let deployments = BlockDeployments::new(&self.deployments, block_number, header_provider, &self.consensus);
				let chain_acceptor = ChainAcceptor::new(
					&tx_out_provider,
					tx_meta_provider,
					header_provider,
					&self.consensus,
					verification_level,
					canon_block,
					block_number,
					median_time_past,
					&deployments,
				);
				chain_acceptor.check()?;
			},
		};

		assert_eq!(Some(self.store.best_block().hash), self.store.block_hash(self.store.best_block().number));
		Ok(())
	}

	pub fn verify_block_header(
		&self,
		_block_header_provider: &dyn BlockHeaderProvider,
		hash: &H256,
		header: &BlockHeader
	) -> Result<(), Error> {
		// let's do only preverifcation
		// TODO: full verification
		let current_time = ::time::get_time().sec as u32;
		let header = IndexedBlockHeader::new(hash.clone(), header.clone());
		let header_verifier = HeaderVerifier::new(&header, self.consensus.network, current_time);
		header_verifier.check()
	}

	pub fn verify_mempool_transaction<T>(
		&self,
		block_header_provider: &dyn BlockHeaderProvider,
		prevout_provider: &T,
		height: u32,
		time: u32,
		transaction: &IndexedTransaction,
	) -> Result<(), TransactionError> where T: TransactionOutputProvider {
		// let's do preverification first
		let deployments = BlockDeployments::new(&self.deployments, height, block_header_provider, &self.consensus);
		let tx_verifier = MemoryPoolTransactionVerifier::new(&transaction, &self.consensus, &deployments);
		try!(tx_verifier.check());

		let canon_tx = CanonTransaction::new(&transaction);
		// now let's do full verification
		let noop = NoopStore;
		let output_store = DuplexTransactionOutputProvider::new(prevout_provider, &noop);
		let previous_block_number = height.checked_sub(1)
			.expect("height is the height of future block of new tx; genesis block can't be in the future; qed");
		let previous_block_header = block_header_provider.block_header(previous_block_number.into())
			.expect("blocks up to height should be in db; qed");
		let median_time_past = median_timestamp_inclusive(previous_block_header.hash, block_header_provider);
		let tx_acceptor = MemoryPoolTransactionAcceptor::new(
			self.store.as_transaction_meta_provider(),
			output_store,
			&self.consensus,
			canon_tx,
			height,
			time,
			median_time_past,
			&deployments,
		);
		tx_acceptor.check()
	}
}

impl Verify for BackwardsCompatibleChainVerifier {
	fn verify(&self, level: VerificationLevel, block: &IndexedBlock) -> Result<(), Error> {
		let result = self.verify_block(level, block);
		trace!(
			target: "verification", "Block {} (transactions: {}) verification finished. Result {:?}",
			block.hash().to_reversed_str(),
			block.transactions.len(),
			result,
		);
		result
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use chain::{IndexedBlock, Transaction, Block};
	use storage::Error as DBError;
	use db::BlockChainDatabase;
	use network::{Network, ConsensusParams, ConsensusFork, BitcoinCashConsensusParams};
	use script;
	use constants::DOUBLE_SPACING_SECONDS;
	use super::BackwardsCompatibleChainVerifier as ChainVerifier;
	use {Verify, Error, TransactionError, VerificationLevel};

	#[test]
	fn verify_orphan() {
		let storage = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let b2 = test_data::block_h2().into();
		let verifier = ChainVerifier::new(storage, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert_eq!(Err(Error::Database(DBError::UnknownParent)), verifier.verify(VerificationLevel::Full, &b2));
	}

	#[test]
	fn verify_smoky() {
		let storage = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let b1 = test_data::block_h1();
		let verifier = ChainVerifier::new(storage, ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert!(verifier.verify(VerificationLevel::Full, &b1.into()).is_ok());
	}

	#[test]
	fn first_tx() {
		let storage = BlockChainDatabase::init_test_chain(
			vec![
				test_data::block_h0().into(),
				test_data::block_h1().into(),
			]);
		let b1 = test_data::block_h2();
		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert!(verifier.verify(VerificationLevel::Full, &b1.into()).is_ok());
	}

	#[test]
	fn coinbase_maturity() {
		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let genesis_coinbase = genesis.transactions()[0].hash();

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(1).build()
				.build()
			.transaction()
				.input().hash(genesis_coinbase).build()
				.output().value(2).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));

		let expected = Err(Error::Transaction(
			1,
			TransactionError::Maturity,
		));

		assert_eq!(expected, verifier.verify(VerificationLevel::Full, &block.into()));
	}

	#[test]
	fn non_coinbase_happy() {
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

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let reference_tx = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(2).build()
				.build()
			.transaction()
				.input().hash(reference_tx).build()
				.output().value(1).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert!(verifier.verify(VerificationLevel::Full, &block.into()).is_ok());
	}

	#[test]
	fn transaction_references_same_block_happy() {
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

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let first_tx_hash = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(2).build()
				.build()
			.transaction()
				.input().hash(first_tx_hash).build()
				.output().value(30).build()
				.output().value(20).build()
				.build()
			.derived_transaction(1, 0)
				.output().value(30).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert!(verifier.verify(VerificationLevel::Full, &block.into()).is_ok());
	}

	#[test]
	fn transaction_references_same_block_overspend() {
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

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let first_tx_hash = genesis.transactions()[1].hash();

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(2).build()
				.build()
			.transaction()
				.input().hash(first_tx_hash).build()
				.output().value(19).build()
				.output().value(31).build()
				.build()
			.derived_transaction(1, 0)
				.output().value(20).build()
				.build()
			.derived_transaction(1, 1)
				.output().value(20).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));

		let expected = Err(Error::Transaction(2, TransactionError::Overspend));
		assert_eq!(expected, verifier.verify(VerificationLevel::Full, &block.into()));
	}

	#[test]
	fn transaction_references_same_block_and_goes_before_previous() {
		let mut blocks = vec![test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build()];
		let input_tx = blocks[0].transactions()[0].clone();
		let mut parent_hash = blocks[0].hash();
		// waiting 100 blocks for genesis coinbase to become valid
		for _ in 0..100 {
			let block: Block = test_data::block_builder()
				.transaction().coinbase().build()
				.merkled_header().parent(parent_hash).build()
				.build()
				.into();
			parent_hash = block.hash();
			blocks.push(block);
		}

		let storage = Arc::new(BlockChainDatabase::init_test_chain(blocks.into_iter().map(Into::into).collect()));

		let tx1: Transaction = test_data::TransactionBuilder::with_version(4)
			.add_input(&input_tx, 0)
			.add_output(10).add_output(10).add_output(10)
			.add_output(5).add_output(5).add_output(5)
			.into();
		let tx2: Transaction = test_data::TransactionBuilder::with_version(1)
			.add_input(&tx1, 0)
			.add_output(1).add_output(1).add_output(1)
			.add_output(2).add_output(2).add_output(2)
			.into();
		assert!(tx1.hash() > tx2.hash());

		let block = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(2).script_pubkey_with_sigops(100).build()
				.build()
			.with_transaction(tx2)
			.with_transaction(tx1)
			.merkled_header()
				.time(DOUBLE_SPACING_SECONDS + 101) // to pass BCH work check
				.parent(parent_hash)
				.build()
			.build();

		// when topological order is required
		let topological_consensus = ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore);
		let verifier = ChainVerifier::new(storage.clone(), topological_consensus);
		let expected = Err(Error::Transaction(1, TransactionError::Overspend));
		assert_eq!(expected, verifier.verify(VerificationLevel::Header, &block.clone().into()));

		// when canonical order is required
		let mut canonical_params = BitcoinCashConsensusParams::new(Network::Unitest);
		canonical_params.magnetic_anomaly_time = 0;
		let canonical_consensus = ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCash(canonical_params));
		let verifier = ChainVerifier::new(storage, canonical_consensus);
		let expected = Ok(());
		assert_eq!(expected, verifier.verify(VerificationLevel::Header, &block.into()));
	}

	#[test]
	#[ignore]
	fn coinbase_happy() {
		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let genesis_coinbase = genesis.transactions()[0].hash();

		// waiting 100 blocks for genesis coinbase to become valid
		for _ in 0..100 {
			let block: IndexedBlock = test_data::block_builder()
				.transaction().coinbase().build()
				.merkled_header().parent(genesis.hash()).build()
				.build()
				.into();
			let hash = block.hash().clone();
			storage.insert(block).expect("All dummy blocks should be inserted");
			storage.canonize(&hash).unwrap();
		}

		let best_hash = storage.best_block().hash;

		let block = test_data::block_builder()
			.transaction().coinbase().build()
			.transaction()
				.input().hash(genesis_coinbase.clone()).build()
				.build()
			.merkled_header().parent(best_hash).build()
			.build();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		assert!(verifier.verify(VerificationLevel::Full, &block.into()).is_ok());
	}

	#[test]
	fn absoulte_sigops_overflow_block() {
		let genesis = test_data::block_builder()
			.transaction()
				.coinbase()
				.build()
			.transaction()
				.output().value(50).build()
				.build()
			.merkled_header().build()
			.build();

		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);
		let reference_tx = genesis.transactions()[1].hash();

		let mut builder_tx1 = script::Builder::default();
		for _ in 0..81000 {
			builder_tx1 = builder_tx1.push_opcode(script::Opcode::OP_CHECKSIG)
		}

		let mut builder_tx2 = script::Builder::default();
		for _ in 0..81001 {
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

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));
		let expected = Err(Error::MaximumSigops);
		assert_eq!(expected, verifier.verify(VerificationLevel::Full, &block.into()));
	}

	#[test]
	fn coinbase_overspend() {
		let genesis = test_data::block_builder()
			.transaction().coinbase().build()
			.merkled_header().build()
			.build();
		let storage = BlockChainDatabase::init_test_chain(vec![genesis.clone().into()]);

		let block: IndexedBlock = test_data::block_builder()
			.transaction()
				.coinbase()
				.output().value(5000000001).build()
				.build()
			.merkled_header().parent(genesis.hash()).build()
			.build()
			.into();

		let verifier = ChainVerifier::new(Arc::new(storage), ConsensusParams::new(Network::Unitest, ConsensusFork::BitcoinCore));

		let expected = Err(Error::CoinbaseOverspend {
			expected_max: 5000000000,
			actual: 5000000001
		});

		assert_eq!(expected, verifier.verify(VerificationLevel::Full, &block.into()));
	}
}
