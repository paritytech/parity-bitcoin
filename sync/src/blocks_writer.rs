use std::sync::Arc;
use chain;
use db;
use network::Magic;
use verification::{Verify, ChainVerifier};
use super::Error;

pub struct BlocksWriter {
	storage: Arc<db::Store>,
	verifier: ChainVerifier,
}

impl BlocksWriter {
	pub fn new(storage: db::SharedStore, network: Magic) -> BlocksWriter {
		BlocksWriter {
			storage: storage.clone(),
			verifier: ChainVerifier::new(storage, network),
		}
	}

	pub fn append_block(&mut self, block: chain::Block) -> Result<(), Error> {
		let indexed_block: db::IndexedBlock = block.into();
		// TODO: share same verification code with synchronization_client
		if self.storage.best_block().map_or(false, |bb| bb.hash != indexed_block.header().previous_header_hash) {
			return Err(Error::OutOfOrderBlock);
		}

		match self.verifier.verify(&indexed_block) {
			Err(err) => Err(Error::Verification(err)),
			Ok(_chain) => { try!(self.storage.insert_indexed_block(&indexed_block).map_err(Error::Database)); Ok(()) }
		}
	}
}

#[cfg(test)]
mod tests {
	use std::sync::Arc;
	use db::{self, Store};
	use network::Magic;
	use {test_data, verification};
	use super::super::Error;
	use super::BlocksWriter;

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
		let mut blocks_target = BlocksWriter::new(db.clone(), Magic::Testnet);
		match blocks_target.append_block(test_data::block_h2()).unwrap_err() {
			Error::OutOfOrderBlock => (),
			_ => panic!("Unexpected error"),
		};
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
			Error::Verification(verification::Error::Empty) => (),
			_ => panic!("Unexpected error"),
		};
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);
	}
}
