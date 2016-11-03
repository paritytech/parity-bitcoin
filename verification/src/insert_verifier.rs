//! Ordered verifier for db insertion

use std::sync::Arc;
use db::{self, BlockRef};
use super::{Verify, VerificationResult, Chain};
use chain;

pub struct InsertVerifier {
	store: Arc<db::Store>,
}

impl InsertVerifier {
	pub fn new(store: Arc<db::Store>) -> InsertVerifier {
		InsertVerifier { store: store }
	}
}

impl Verify for InsertVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {
		// todo more ordered verification

		let _parent_header = match self.store.block_header_bytes(BlockRef::Hash(block.header().previous_header_hash.clone())) {
			Some(b) => b,
			None => { return Ok(Chain::Orphan); }
		};

		Ok(Chain::Main)
	}
}
