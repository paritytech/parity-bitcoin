#[macro_use]
extern crate lazy_static;

extern crate chain;
extern crate primitives;

mod consensus;
mod deployments;
mod network;

pub use primitives::{hash, compact};

pub use consensus::{ConsensusParams, ConsensusFork, BitcoinCashConsensusParams, TransactionOrdering};
pub use deployments::Deployment;
pub use network::{Magic, Network};
