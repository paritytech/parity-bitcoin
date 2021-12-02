extern crate bitcrypto as crypto;
extern crate ethereum_types;
extern crate heapsize;
extern crate primitives;
extern crate rayon;
extern crate rustc_hex as hex;
extern crate serialization as ser;
#[macro_use]
extern crate serialization_derive;

pub use block::Block;
pub use block_header::BlockHeader;
pub use indexed_block::IndexedBlock;
pub use indexed_header::IndexedBlockHeader;
pub use indexed_transaction::IndexedTransaction;
pub use merkle_root::{merkle_node_hash, merkle_root};
pub use primitives::{bytes, compact, hash};
pub use read_and_hash::{HashedData, ReadAndHash};
pub use transaction::{OutPoint, Transaction, TransactionInput, TransactionOutput};

pub mod constants;

mod block;
mod block_header;
mod merkle_root;
mod transaction;

/// `IndexedBlock` extension
mod read_and_hash;
mod indexed_block;
mod indexed_header;
mod indexed_transaction;

pub type ShortTransactionID = hash::H48;
