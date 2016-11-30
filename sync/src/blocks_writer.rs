use std::sync::Arc;
use std::collections::VecDeque;
use parking_lot::Mutex;
use chain;
use db;
use network::Magic;
use orphan_blocks_pool::OrphanBlocksPool;
use synchronization_verifier::{Verifier, SyncVerifier, VerificationSink};
use primitives::hash::H256;
use super::Error;

pub const MAX_ORPHANED_BLOCKS: usize = 64;

pub struct BlocksWriter {
	storage: db::SharedStore,
	orphaned_blocks_pool: OrphanBlocksPool,
	verifier: SyncVerifier<BlocksWriterSink>,
	sink: Arc<Mutex<BlocksWriterSink>>,
}

struct BlocksWriterSink {
	storage: db::SharedStore,
	err: Option<Error>,
}

impl BlocksWriter {
	pub fn new(storage: db::SharedStore, network: Magic) -> BlocksWriter {
		let sink = Arc::new(Mutex::new(BlocksWriterSink::new(storage.clone())));
		let verifier = SyncVerifier::new(network, storage.clone(), sink.clone());
		BlocksWriter {
			storage: storage,
			orphaned_blocks_pool: OrphanBlocksPool::new(),
			verifier: verifier,
			sink: sink,
		}
	}

	pub fn append_block(&mut self, block: chain::Block) -> Result<(), Error> {
		let indexed_block: db::IndexedBlock = block.into();
		// verify && insert only if parent block is already in the storage
		if !self.storage.contains_block(db::BlockRef::Hash(indexed_block.header().previous_header_hash.clone())) {
			self.orphaned_blocks_pool.insert_orphaned_block(indexed_block.hash().clone(), indexed_block);
			// we can't hold many orphaned blocks in memory during import
			if self.orphaned_blocks_pool.len() > MAX_ORPHANED_BLOCKS {
				return Err(Error::TooManyOrphanBlocks);
			}
			return Ok(());
		}

		// verify && insert block && all its orphan children
		let mut verification_queue: VecDeque<db::IndexedBlock> = self.orphaned_blocks_pool.remove_blocks_for_parent(indexed_block.hash()).into_iter().map(|(_, b)| b).collect();
		verification_queue.push_front(indexed_block);
		while let Some(block) = verification_queue.pop_front() {
			println!("Verifying {:?}", block.hash().to_reversed_str());
			self.verifier.verify_block(block);
			if let Some(err) = self.sink.lock().error() {
				return Err(err);
			}
		}

		Ok(())
	}
}

impl BlocksWriterSink {
	pub fn new(storage: db::SharedStore) -> Self {
		BlocksWriterSink {
			storage: storage,
			err: None,
		}
	}

	pub fn error(&mut self) -> Option<Error> {
		self.err.take()
	}
}

impl VerificationSink for BlocksWriterSink {
	fn on_block_verification_success(&mut self, block: db::IndexedBlock) {
		if let Err(err) = self.storage.insert_indexed_block(&block) {
			self.err = Some(Error::Database(err));
		}
	}

	fn on_block_verification_error(&mut self, err: &str, _hash: &H256) {
		self.err = Some(Error::Verification(err.into()));
	}

	fn on_transaction_verification_success(&mut self, _transaction: chain::Transaction) {
		unreachable!("not intended to verify transactions")
	}

	fn on_transaction_verification_error(&mut self, _err: &str, _hash: &H256) {
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
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet);
		blocks_target.append_block(test_data::block_h1()).expect("Expecting no error");
		assert_eq!(db.best_block().expect("Block is inserted").number, 1);
	}

	#[test]
	fn blocks_writer_verification_error() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let blocks = test_data::build_n_empty_blocks_from_genesis((MAX_ORPHANED_BLOCKS + 2) as u32, 1);
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet);
		for (index, block) in blocks.into_iter().skip(1).enumerate() {
			match blocks_target.append_block(block) {
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
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet);

		let wrong_block = test_data::block_builder()
			.header().parent(test_data::genesis().hash()).build()
		.build();
		match blocks_target.append_block(wrong_block).unwrap_err() {
			Error::Verification(_) => (),
			_ => panic!("Unexpected error"),
		};
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);
	}
}
