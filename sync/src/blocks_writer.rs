use std::sync::Arc;
use std::collections::VecDeque;
use parking_lot::Mutex;
use chain;
use db;
use network::Magic;
use orphan_blocks_pool::OrphanBlocksPool;
use synchronization_verifier::{Verifier, SyncVerifier, VerificationTask,
	VerificationSink, BlockVerificationSink, TransactionVerificationSink};
use primitives::hash::H256;
use super::Error;

pub const MAX_ORPHANED_BLOCKS: usize = 1024;

pub struct BlocksWriter {
	storage: db::SharedStore,
	orphaned_blocks_pool: OrphanBlocksPool,
	verifier: SyncVerifier<BlocksWriterSink>,
	sink: Arc<BlocksWriterSinkData>,
	verification: bool,
}

struct BlocksWriterSink {
	data: Arc<BlocksWriterSinkData>,
}

struct BlocksWriterSinkData {
	storage: db::SharedStore,
	err: Mutex<Option<Error>>,
}

impl BlocksWriter {
	pub fn new(storage: db::SharedStore, network: Magic, verification: bool) -> BlocksWriter {
		let sink_data = Arc::new(BlocksWriterSinkData::new(storage.clone()));
		let sink = Arc::new(BlocksWriterSink::new(sink_data.clone()));
		let verifier = SyncVerifier::new(network, storage.clone(), sink);
		BlocksWriter {
			storage: storage,
			orphaned_blocks_pool: OrphanBlocksPool::new(),
			verifier: verifier,
			sink: sink_data,
			verification: verification,
		}
	}

	pub fn append_block(&mut self, block: chain::IndexedBlock) -> Result<(), Error> {
		// do not append block if it is already there
		if self.storage.contains_block(db::BlockRef::Hash(block.hash().clone())) {
			return Ok(());
		}
		// verify && insert only if parent block is already in the storage
		if !self.storage.contains_block(db::BlockRef::Hash(block.header.raw.previous_header_hash.clone())) {
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
			if self.verification {
				self.verifier.verify_block(block);

				if let Some(err) = self.sink.error() {
					return Err(err);
				}
			} else {
				try!(self.storage.insert_indexed_block(&block).map_err(Error::Database));
			}
		}

		Ok(())
	}
}

impl BlocksWriterSink {
	pub fn new(data: Arc<BlocksWriterSinkData>) -> Self {
		BlocksWriterSink {
			data: data,
		}
	}
}

impl BlocksWriterSinkData {
	pub fn new(storage: db::SharedStore) -> Self {
		BlocksWriterSinkData {
			storage: storage,
			err: Mutex::new(None),
		}
	}

	pub fn error(&self) -> Option<Error> {
		self.err.lock().take()
	}
}

impl VerificationSink for BlocksWriterSink {
}

impl BlockVerificationSink for BlocksWriterSink {
	fn on_block_verification_success(&self, block: chain::IndexedBlock) -> Option<Vec<VerificationTask>> {
		if let Err(err) = self.data.storage.insert_indexed_block(&block) {
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
	use std::sync::Arc;
	use db::{self, Store};
	use network::Magic;
	use test_data;
	use super::super::Error;
	use super::{BlocksWriter, MAX_ORPHANED_BLOCKS};

	#[test]
	fn blocks_writer_appends_blocks() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet, true);
		blocks_target.append_block(test_data::block_h1().into()).expect("Expecting no error");
		assert_eq!(db.best_block().expect("Block is inserted").number, 1);
	}

	#[test]
	fn blocks_writer_verification_error() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let blocks = test_data::build_n_empty_blocks_from_genesis((MAX_ORPHANED_BLOCKS + 2) as u32, 1);
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet, true);
		for (index, block) in blocks.into_iter().skip(1).enumerate() {
			match blocks_target.append_block(block.into()) {
				Err(Error::TooManyOrphanBlocks) if index == MAX_ORPHANED_BLOCKS => (),
				Ok(_) if index != MAX_ORPHANED_BLOCKS => (),
				_ => panic!("unexpected"),
			}
		}
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);
	}

	#[test]
	fn blocks_writer_out_of_order_block() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet, true);

		let wrong_block = test_data::block_builder()
			.header().parent(test_data::genesis().hash()).build()
		.build();
		match blocks_target.append_block(wrong_block.into()).unwrap_err() {
			Error::Verification(_) => (),
			_ => panic!("Unexpected error"),
		};
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);
	}

	#[test]
	fn blocks_writer_append_to_existing_db() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet, true);

		assert!(blocks_target.append_block(test_data::genesis().into()).is_ok());
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);

		assert!(blocks_target.append_block(test_data::block_h1().into()).is_ok());
		assert_eq!(db.best_block().expect("Block is inserted").number, 1);
	}
}
