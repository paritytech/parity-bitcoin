extern crate rustc_serialize;
extern crate heapsize;
extern crate primitives;
extern crate bitcrypto as crypto;
extern crate serialization as ser;

mod block;
mod block_header;
mod merkle_root;
mod transaction;

pub trait RepresentH256 {
	fn h256(&self) -> primitives::hash::H256;
}

pub use rustc_serialize::hex;
pub use primitives::{hash, bytes, uint};

pub use self::block::Block;
pub use self::block_header::BlockHeader;
pub use self::merkle_root::merkle_root;
pub use self::merkle_root::merkle_node_hash;
pub use self::transaction::{
	Transaction, TransactionInput, TransactionOutput, OutPoint,
	SEQUENCE_LOCKTIME_DISABLE_FLAG, SEQUENCE_FINAL,
	SEQUENCE_LOCKTIME_TYPE_FLAG, SEQUENCE_LOCKTIME_MASK
};

pub type ShortTransactionID = hash::H48;
