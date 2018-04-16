use std::collections::VecDeque;
use std::sync::Arc;
use parking_lot::Mutex;
use chain;
use storage;
use network::ConsensusParams;
use primitives::hash::H256;
use super::Error;
use synchronization_verifier::{Verifier, SyncVerifier, VerificationTask,
	VerificationSink, BlockVerificationSink, TransactionVerificationSink};
use types::StorageRef;
use utils::OrphanBlocksPool;
use VerificationParameters;

/// Maximum number of orphaned in-memory blocks
pub const MAX_ORPHANED_BLOCKS: usize = 1024;

/// Synchronous block writer
pub struct BlocksWriter {
	/// Blocks storage
	storage: StorageRef,
	/// Orphaned blocks pool
	orphaned_blocks_pool: OrphanBlocksPool,
	/// Blocks verifier
	verifier: SyncVerifier<BlocksWriterSink>,
	/// Verification events receiver
	sink: Arc<BlocksWriterSinkData>,
}

/// Verification events receiver
struct BlocksWriterSink {
	/// Reference to blocks writer data
	data: Arc<BlocksWriterSinkData>,
}

/// Blocks writer data
struct BlocksWriterSinkData {
	/// Blocks storage
	storage: StorageRef,
	/// Last verification error
	err: Mutex<Option<Error>>,
}

impl BlocksWriter {
	/// Create new synchronous blocks writer
	pub fn new(storage: StorageRef, consensus: ConsensusParams, verification_params: VerificationParameters) -> BlocksWriter {
		let sink_data = Arc::new(BlocksWriterSinkData::new(storage.clone()));
		let sink = Arc::new(BlocksWriterSink::new(sink_data.clone()));
		let verifier = SyncVerifier::new(consensus, storage.clone(), sink, verification_params);
		BlocksWriter {
			storage: storage,
			orphaned_blocks_pool: OrphanBlocksPool::new(),
			verifier: verifier,
			sink: sink_data,
		}
	}

	/// Append new block
	pub fn append_block(&mut self, block: chain::IndexedBlock) -> Result<(), Error> {
		// do not append block if it is already there
		if self.storage.contains_block(storage::BlockRef::Hash(block.hash().clone())) {
			return Ok(());
		}

		// verify && insert only if parent block is already in the storage
		if !self.storage.contains_block(storage::BlockRef::Hash(block.header.raw.previous_header_hash.clone())) {
			self.orphaned_blocks_pool.insert_orphaned_block(block);
			// we can't hold many orphaned blocks in memory during import
			if self.orphaned_blocks_pool.len() > MAX_ORPHANED_BLOCKS {
				return Err(Error::TooManyOrphanBlocks);
			}
			return Ok(());
		}

		// verify && insert block && all its orphan children
		let mut verification_queue: VecDeque<chain::IndexedBlock> = self.orphaned_blocks_pool.remove_blocks_for_parent(block.hash());
		verification_queue.push_front(block);
		while let Some(block) = verification_queue.pop_front() {
			self.verifier.verify_block(block);
			if let Some(err) = self.sink.error() {
				return Err(err);
			}
		}

		Ok(())
	}
}

impl BlocksWriterSink {
	/// Create new verification events receiver
	pub fn new(data: Arc<BlocksWriterSinkData>) -> Self {
		BlocksWriterSink {
			data: data,
		}
	}
}

impl BlocksWriterSinkData {
	/// Create new blocks writer data
	pub fn new(storage: StorageRef) -> Self {
		BlocksWriterSinkData {
			storage: storage,
			err: Mutex::new(None),
		}
	}

	/// Take last verification error
	pub fn error(&self) -> Option<Error> {
		self.err.lock().take()
	}
}

impl VerificationSink for BlocksWriterSink {
}

impl BlockVerificationSink for BlocksWriterSink {
	fn on_block_verification_success(&self, block: chain::IndexedBlock) -> Option<Vec<VerificationTask>> {
		let hash = block.hash().clone();
		if let Err(err) = self.data.storage.insert(block) {
			*self.data.err.lock() = Some(Error::Database(err));
		}
		if let Err(err) = self.data.storage.canonize(&hash) {
			*self.data.err.lock() = Some(Error::Database(err));
		}

		None
	}

	fn on_block_verification_error(&self, err: &str, _hash: &H256) {
		*self.data.err.lock() = Some(Error::Verification(err.into()));
	}
}

impl TransactionVerificationSink for BlocksWriterSink {
	fn on_transaction_verification_success(&self, _transaction: chain::IndexedTransaction) {
		unreachable!("not intended to verify transactions")
	}

	fn on_transaction_verification_error(&self, _err: &str, _hash: &H256) {
		unreachable!("not intended to verify transactions")
	}
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::sync::Arc;
	use db::{BlockChainDatabase};
	use network::{ConsensusParams, ConsensusFork, Network};
	use verification::VerificationLevel;
	use super::super::Error;
	use super::{BlocksWriter, MAX_ORPHANED_BLOCKS};
	use VerificationParameters;

	fn default_verification_params() -> VerificationParameters {
		VerificationParameters {
			verification_level: VerificationLevel::Full,
			verification_edge: 0u8.into(),
		}
	}

	#[test]
	fn blocks_writer_appends_blocks() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut blocks_target = BlocksWriter::new(db.clone(), ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore), default_verification_params());
		blocks_target.append_block(test_data::block_h1().into()).expect("Expecting no error");
		assert_eq!(db.best_block().number, 1);
	}

	#[test]
	fn blocks_writer_verification_error() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let blocks = test_data::build_n_empty_blocks_from_genesis((MAX_ORPHANED_BLOCKS + 2) as u32, 1);
		let mut blocks_target = BlocksWriter::new(db.clone(), ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore), default_verification_params());
		for (index, block) in blocks.into_iter().skip(1).enumerate() {
			match blocks_target.append_block(block.into()) {
				Err(Error::TooManyOrphanBlocks) if index == MAX_ORPHANED_BLOCKS => (),
				Ok(_) if index != MAX_ORPHANED_BLOCKS => (),
				_ => panic!("unexpected"),
			}
		}
		assert_eq!(db.best_block().number, 0);
	}

	#[test]
	fn blocks_writer_out_of_order_block() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut blocks_target = BlocksWriter::new(db.clone(), ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore), default_verification_params());

		let wrong_block = test_data::block_builder()
			.header().parent(test_data::genesis().hash()).build()
		.build();
		match blocks_target.append_block(wrong_block.into()).unwrap_err() {
			Error::Verification(_) => (),
			_ => panic!("Unexpected error"),
		};
		assert_eq!(db.best_block().number, 0);
	}

	#[test]
	fn blocks_writer_append_to_existing_db() {
		let db = Arc::new(BlockChainDatabase::init_test_chain(vec![test_data::genesis().into()]));
		let mut blocks_target = BlocksWriter::new(db.clone(), ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore), default_verification_params());

		assert!(blocks_target.append_block(test_data::genesis().into()).is_ok());
		assert_eq!(db.best_block().number, 0);

		assert!(blocks_target.append_block(test_data::block_h1().into()).is_ok());
		assert_eq!(db.best_block().number, 1);
	}
}
