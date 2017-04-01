//! Bitcoin chain verifier

use hash::H256;
use chain::{IndexedBlock, IndexedBlockHeader, BlockHeader, Transaction};
use db::{
	SharedStore, PreviousTransactionOutputProvider, BlockHeaderProvider, TransactionOutputObserver,
	BlockOrigin
};
use network::Magic;
use error::{Error, TransactionError};
use canon::{CanonBlock, CanonTransaction};
use duplex_store::{DuplexTransactionOutputObserver, DuplexTransactionOutputProvider, NoopStore};
use verify_chain::ChainVerifier;
use verify_header::HeaderVerifier;
use verify_transaction::MemoryPoolTransactionVerifier;
use accept_chain::ChainAcceptor;
use accept_transaction::MemoryPoolTransactionAcceptor;
use Verify;

pub struct BackwardsCompatibleChainVerifier {
	store: SharedStore,
	network: Magic,
}

impl BackwardsCompatibleChainVerifier {
	pub fn new(store: SharedStore, network: Magic) -> Self {
		BackwardsCompatibleChainVerifier {
			store: store,
			network: network,
		}
	}

	fn verify_block(&self, block: &IndexedBlock) -> Result<(), Error> {
		let current_time = ::time::get_time().sec as u32;
		// first run pre-verification
		let chain_verifier = ChainVerifier::new(block, self.network, current_time);
		chain_verifier.check()?;

		assert_eq!(Some(self.store.best_block().hash), self.store.block_hash(self.store.best_block().number));
		let block_origin = self.store.block_origin(&block.header)?;
		trace!(target: "verification", "verify_block: {:?} best_block: {:?} block_origin: {:?}", block.hash().reversed(), self.store.best_block(), block_origin);
		match block_origin {
			BlockOrigin::KnownBlock => {
				// there should be no known blocks at this point
				unreachable!();
			},
			BlockOrigin::CanonChain { block_number } => {
				let canon_block = CanonBlock::new(block);
				let chain_acceptor = ChainAcceptor::new(self.store.as_store(), self.network, canon_block, block_number);
				chain_acceptor.check()?;
			},
			BlockOrigin::SideChain(origin) => {
				let block_number = origin.block_number;
				let fork = self.store.fork(origin)?;
				let canon_block = CanonBlock::new(block);
				let chain_acceptor = ChainAcceptor::new(fork.store(), self.network, canon_block, block_number);
				chain_acceptor.check()?;
			},
			BlockOrigin::SideChainBecomingCanonChain(origin) => {
				let block_number = origin.block_number;
				let fork = self.store.fork(origin)?;
				let canon_block = CanonBlock::new(block);
				let chain_acceptor = ChainAcceptor::new(fork.store(), self.network, canon_block, block_number);
				chain_acceptor.check()?;
			},
		}

		assert_eq!(Some(self.store.best_block().hash), self.store.block_hash(self.store.best_block().number));
		Ok(())
	}

	pub fn verify_block_header(
		&self,
		_block_header_provider: &BlockHeaderProvider,
		hash: &H256,
		header: &BlockHeader
	) -> Result<(), Error> {
		// let's do only preverifcation
		// TODO: full verification
		let current_time = ::time::get_time().sec as u32;
		let header = IndexedBlockHeader::new(hash.clone(), header.clone());
		let header_verifier = HeaderVerifier::new(&header, self.network, current_time);
		header_verifier.check()
	}

	pub fn verify_mempool_transaction<T>(
		&self,
		prevout_provider: &T,
		height: u32,
		time: u32,
		transaction: &Transaction,
	) -> Result<(), TransactionError> where T: PreviousTransactionOutputProvider + TransactionOutputObserver {
		let indexed_tx = transaction.clone().into();
		// let's do preverification first
		let tx_verifier = MemoryPoolTransactionVerifier::new(&indexed_tx);
		try!(tx_verifier.check());

		let canon_tx = CanonTransaction::new(&indexed_tx);
		// now let's do full verification
		let noop = NoopStore;
		let prevouts = DuplexTransactionOutputProvider::new(prevout_provider, &noop);
		let spents = DuplexTransactionOutputObserver::new(prevout_provider, &noop);
		let tx_acceptor = MemoryPoolTransactionAcceptor::new(
			self.store.as_transaction_meta_provider(),
			prevouts,
			spents,
			self.network,
			canon_tx,
			height,
			time
		);
		tx_acceptor.check()
	}
}

impl Verify for BackwardsCompatibleChainVerifier {
	fn verify(&self, block: &IndexedBlock) -> Result<(), Error> {
		let result = self.verify_block(block);
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
	use chain::{Block, IndexedBlock};
	use db::{SharedStore, BlockChainDatabase, Error as DBError};
	use db::kv::MemoryDatabase;
	//use db::{TestStorage, Storage, Store, BlockStapler};
	use network::Magic;
	use super::BackwardsCompatibleChainVerifier as ChainVerifier;
	use {Verify, Error, TransactionError};

	fn create_test_storage(blocks: Vec<Block>) -> SharedStore {
		let blocks = blocks.into_iter().map(Into::into).collect::<Vec<IndexedBlock>>();
		let store = BlockChainDatabase::open(MemoryDatabase::default());

		for block in &blocks {
			store.insert(block).unwrap();
			store.canonize(block.hash()).unwrap();
		}
		Arc::new(store)
	}

	#[test]
	fn verify_orphan() {
		let storage = create_test_storage(vec![test_data::genesis()]);
		let b2 = test_data::block_h2().into();
		let verifier = ChainVerifier::new(storage, Magic::Unitest);
		assert_eq!(Err(Error::Database(DBError::UnknownParent)), verifier.verify(&b2));
	}

	#[test]
	fn verify_smoky() {
		let storage = create_test_storage(vec![test_data::genesis()]);
		let b1 = test_data::block_h1();
		let verifier = ChainVerifier::new(storage, Magic::Unitest);
		assert!(verifier.verify(&b1.into()).is_ok());
	}

}

	//#[test]
	//fn firtst_tx() {
		//let storage = TestStorage::with_blocks(
			//&vec![
				//test_data::block_h9(),
				//test_data::block_h169(),
			//]
		//);
		//let b1 = test_data::block_h170();
		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);
		//assert_eq!(Chain::Main, verifier.verify(&b1.into()).unwrap());
	//}

	//#[test]
	//fn coinbase_maturity() {

		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).unwrap();
		//let genesis_coinbase = genesis.transactions()[0].hash();

		//let block = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(1).build()
				//.build()
			//.transaction()
				//.input().hash(genesis_coinbase).build()
				//.output().value(2).build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Err(Error::Transaction(
			//1,
			//TransactionError::Maturity,
		//));

		//assert_eq!(expected, verifier.verify(&block.into()));
	//}

	//#[test]
	//fn non_coinbase_happy() {
		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(1).build()
				//.build()
			//.transaction()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).unwrap();
		//let reference_tx = genesis.transactions()[1].hash();

		//let block = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(2).build()
				//.build()
			//.transaction()
				//.input().hash(reference_tx).build()
				//.output().value(1).build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Ok(Chain::Main);
		//assert_eq!(expected, verifier.verify(&block.into()));
	//}


	//#[test]
	//fn transaction_references_same_block_happy() {
		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(1).build()
				//.build()
			//.transaction()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).expect("Genesis should be inserted with no errors");
		//let first_tx_hash = genesis.transactions()[1].hash();

		//let block = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(2).build()
				//.build()
			//.transaction()
				//.input().hash(first_tx_hash).build()
				//.output().value(30).build()
				//.output().value(20).build()
				//.build()
			//.derived_transaction(1, 0)
				//.output().value(30).build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Ok(Chain::Main);
		//assert_eq!(expected, verifier.verify(&block.into()));
	//}

	//#[test]
	//fn transaction_references_same_block_overspend() {
		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(1).build()
				//.build()
			//.transaction()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).expect("Genesis should be inserted with no errors");
		//let first_tx_hash = genesis.transactions()[1].hash();

		//let block = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(2).build()
				//.build()
			//.transaction()
				//.input().hash(first_tx_hash).build()
				//.output().value(19).build()
				//.output().value(31).build()
				//.build()
			//.derived_transaction(1, 0)
				//.output().value(20).build()
				//.build()
			//.derived_transaction(1, 1)
				//.output().value(20).build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Err(Error::Transaction(2, TransactionError::Overspend));
		//assert_eq!(expected, verifier.verify(&block.into()));
	//}

	//#[test]
	//#[ignore]
	//fn coinbase_happy() {

		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).unwrap();
		//let genesis_coinbase = genesis.transactions()[0].hash();

		//// waiting 100 blocks for genesis coinbase to become valid
		//for _ in 0..100 {
			//storage.insert_block(
				//&test_data::block_builder()
					//.transaction().coinbase().build()
				//.merkled_header().parent(genesis.hash()).build()
				//.build()
			//).expect("All dummy blocks should be inserted");
		//}

		//let best_hash = storage.best_block().expect("Store should have hash after all we pushed there").hash;

		//let block = test_data::block_builder()
			//.transaction().coinbase().build()
			//.transaction()
				//.input().hash(genesis_coinbase.clone()).build()
				//.build()
			//.merkled_header().parent(best_hash).build()
			//.build();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Ok(Chain::Main);

		//assert_eq!(expected, verifier.verify(&block.into()))
	//}

	//#[test]
	//fn sigops_overflow_block() {
		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.build()
			//.transaction()
				//.output().value(50).build()
				//.build()
			//.merkled_header().build()
			//.build();

		//storage.insert_block(&genesis).unwrap();
		//let reference_tx = genesis.transactions()[1].hash();

		//let mut builder_tx1 = script::Builder::default();
		//for _ in 0..11000 {
			//builder_tx1 = builder_tx1.push_opcode(script::Opcode::OP_CHECKSIG)
		//}

		//let mut builder_tx2 = script::Builder::default();
		//for _ in 0..11001 {
			//builder_tx2 = builder_tx2.push_opcode(script::Opcode::OP_CHECKSIG)
		//}

		//let block: IndexedBlock = test_data::block_builder()
			//.transaction().coinbase().build()
			//.transaction()
				//.input()
					//.hash(reference_tx.clone())
					//.signature_bytes(builder_tx1.into_script().to_bytes())
					//.build()
				//.build()
			//.transaction()
				//.input()
					//.hash(reference_tx)
					//.signature_bytes(builder_tx2.into_script().to_bytes())
					//.build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build()
			//.into();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Err(Error::MaximumSigops);
		//assert_eq!(expected, verifier.verify(&block.into()));
	//}

	//#[test]
	//fn coinbase_overspend() {

		//let path = RandomTempPath::create_dir();
		//let storage = Storage::new(path.as_path()).unwrap();

		//let genesis = test_data::block_builder()
			//.transaction().coinbase().build()
			//.merkled_header().build()
			//.build();
		//storage.insert_block(&genesis).unwrap();

		//let block: IndexedBlock = test_data::block_builder()
			//.transaction()
				//.coinbase()
				//.output().value(5000000001).build()
				//.build()
			//.merkled_header().parent(genesis.hash()).build()
			//.build()
			//.into();

		//let verifier = ChainVerifier::new(Arc::new(storage), Magic::Unitest);

		//let expected = Err(Error::CoinbaseOverspend {
			//expected_max: 5000000000,
			//actual: 5000000001
		//});

		//assert_eq!(expected, verifier.verify(&block.into()));
	//}
//}
