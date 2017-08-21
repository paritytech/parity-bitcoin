extern crate chain;
extern crate primitives;
extern crate serialization as ser;

mod consensus;
mod deployments;
mod magic;

pub use primitives::{hash, compact};

pub use consensus::{ConsensusParams, ConsensusFork, SEGWIT2X_FORK_BLOCK, BITCOIN_CASH_FORK_BLOCK, segwit};
pub use deployments::Deployment;
pub use magic::Magic;

