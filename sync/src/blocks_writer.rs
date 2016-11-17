use std::sync::Arc;
use chain;
use db;
use super::Error;
use verification::{Verify, ChainVerifier};

pub struct BlocksWriter {
	storage: Arc<db::Store>,
	verifier: ChainVerifier,
}

impl BlocksWriter {
	pub fn new(storage: Arc<db::Store>) -> BlocksWriter {
		BlocksWriter {
			storage: storage.clone(),
			verifier: ChainVerifier::new(storage),
		}
	}

	pub fn append_block(&mut self, block: chain::Block) -> Result<(), Error> {
		// TODO: share same verification code with synchronization_client
		if self.storage.best_block().map_or(false, |bb| bb.hash != block.block_header.previous_header_hash) {
			return Err(Error::OutOfOrderBlock);
		}

		match self.verifier.verify(&block) {
			Err(err) => Err(Error::Verification(err)),
			Ok(_chain) => { try!(self.storage.insert_block(&block).map_err(Error::Database)); Ok(()) }
		}
	}
}

#[cfg(test)]
mod tests {
	use db;
	use db::Store;
	use std::sync::Arc;
	use super::super::Error;
	use super::BlocksWriter;
	use chain::RepresentH256;
	use test_data;
	use verification;

	#[test]
	fn blocks_writer_appends_blocks() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone());
		blocks_target.append_block(test_data::block_h1()).expect("Expecting no error");
		assert_eq!(db.best_block().expect("Block is inserted").number, 1);
	}

	#[test]
	fn blocks_writer_verification_error() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone());
		match blocks_target.append_block(test_data::block_h2()).unwrap_err() {
			Error::OutOfOrderBlock => (),
			_ => panic!("Unexpected error"),
		};
		assert_eq!(db.best_block().expect("Block is inserted").number, 0);
	}

	#[test]
	fn blocks_writer_out_of_order_block() {
		let db = Arc::new(db::TestStorage::with_genesis_block());
		let mut blocks_target = BlocksWriter::new(db.clone());

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
