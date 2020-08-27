extern crate chain;
extern crate ethereum_types;
#[macro_use]
extern crate lazy_static;
extern crate primitives;

pub use consensus::{BitcoinCashConsensusParams, ConsensusFork, ConsensusParams, TransactionOrdering};
pub use deployments::Deployment;
pub use network::{Magic, Network};
pub use primitives::{compact, hash};

mod consensus;
mod deployments;
mod network;

