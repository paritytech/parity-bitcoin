extern crate chain;
extern crate primitives;
extern crate serialization as ser;

mod consensus;
mod magic;

pub use primitives::hash;

pub use consensus::ConsensusParams;
pub use magic::Magic;

