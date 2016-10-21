//! Bitcoin chain verifier

use std::sync::Arc;

use db;
use chain;
use super::{Verify, VerificationResult, Chain};

struct ChainVerifier {
	store: Arc<db::Store>,
}

impl Verify for ChainVerifier {
	fn verify(&self, block: &chain::Block) -> VerificationResult {
		Ok(Chain::Main)
	}
}
