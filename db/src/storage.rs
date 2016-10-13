//! Bitcoin storage

use kvdb::Database;
use primitives::hash::H256;
use super::{BlockRef, Bytes};

const COL_COUNT: u32 = 10;
const COL_META: u32 = 0;
const COL_BLOCK_HASHES: u32 = 1;
const COL_BLOCK_HEADERS: u32 = 2;
const COL_BLOCK_BODIES: u32 = 3;
const COL_BLOCK_TRANSACTIONS: u32 = 4;
const COL_TRANSACTIONS: u32 = 5;
const COL_RESERVED1: u32 = 6;
const COL_RESERVED2: u32 = 7;
const COL_RESERVED3: u32 = 8;
const COL_RESERVED4: u32 = 9;
const COL_RESERVED5: u32 = 10;

pub trait Store {
	fn block_hash(number: u64) -> Option<H256>;

	fn block_header(block_ref: BlockRef) -> Option<Bytes>;

	fn block_body(block_ref: BlockRef) -> Option<Bytes>;

	fn block_transactions(block_ref: BlockRef) -> Vec<H256>;

	fn transaction(hash: H256) -> Option<Bytes>;
}

struct Storage {
	database: Database,
}

impl Store for Storage {
	fn block_hash(number: u64) -> Option<H256> { None }

	fn block_header(block_ref: BlockRef) -> Option<Bytes> { None }

	fn block_body(block_ref: BlockRef) -> Option<Bytes> { None }

	fn block_transactions(block_ref: BlockRef) -> Vec<H256> { Vec::new() }

	fn transaction(hash: H256) -> Option<Bytes> { None }
}
