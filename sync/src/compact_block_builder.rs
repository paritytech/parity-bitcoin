use std::collections::HashSet;
use rand::{thread_rng, Rng};
use bitcrypto::{sha256, siphash24};
use byteorder::{LittleEndian, ByteOrder};
use chain::{BlockHeader, ShortTransactionID};
use db::IndexedBlock;
use message::common::{BlockHeaderAndIDs, PrefilledTransaction};
use primitives::hash::H256;
use ser::{Stream, Serializable};

/// Maximum size of prefilled transactions in compact block
const MAX_COMPACT_BLOCK_PREFILLED_SIZE: usize = 10 * 1024;

pub fn build_compact_block(block: IndexedBlock, prefilled_transactions_indexes: HashSet<usize>) -> BlockHeaderAndIDs {
	let nonce: u64 = thread_rng().gen();

	let prefilled_transactions_len = prefilled_transactions_indexes.len();
	let mut short_ids: Vec<ShortTransactionID> = Vec::with_capacity(block.transactions_len() - prefilled_transactions_len);
	let mut prefilled_transactions: Vec<PrefilledTransaction> = Vec::with_capacity(prefilled_transactions_len);
	let mut prefilled_transactions_size: usize = 0;

	for (transaction_index, (transaction_hash, transaction)) in block.transactions().enumerate() {
		let transaction_size = transaction.serialized_size();
		if prefilled_transactions_size + transaction_size < MAX_COMPACT_BLOCK_PREFILLED_SIZE
			&& prefilled_transactions_indexes.contains(&transaction_index) {
			prefilled_transactions_size += transaction_size;
			prefilled_transactions.push(PrefilledTransaction {
				index: transaction_index,
				transaction: transaction.clone(),
			})
		} else {
			short_ids.push(short_transaction_id(nonce, block.header(), transaction_hash));
		}
	}

	BlockHeaderAndIDs {
		header: block.header().clone(),
		nonce: nonce,
		short_ids: short_ids,
		prefilled_transactions: prefilled_transactions,
	}
}

fn short_transaction_id(nonce: u64, block_header: &BlockHeader, transaction_hash: &H256) -> ShortTransactionID {
	// Short transaction IDs are used to represent a transaction without sending a full 256-bit hash. They are calculated by:
	// 1) single-SHA256 hashing the block header with the nonce appended (in little-endian)
	let mut stream = Stream::new();
	stream.append(block_header);
	stream.append(&nonce);
	let block_header_with_nonce_hash = sha256(&stream.out());

	// 2) Running SipHash-2-4 with the input being the transaction ID and the keys (k0/k1) set to the first two little-endian
	// 64-bit integers from the above hash, respectively.
	let key0 = LittleEndian::read_u64(&block_header_with_nonce_hash[0..8]);
	let key1 = LittleEndian::read_u64(&block_header_with_nonce_hash[8..16]);
	let siphash_transaction_hash = siphash24(key0, key1, &**transaction_hash);

	// 3) Dropping the 2 most significant bytes from the SipHash output to make it 6 bytes.
	let mut siphash_transaction_hash_bytes = [0u8; 8];
	LittleEndian::write_u64(&mut siphash_transaction_hash_bytes, siphash_transaction_hash);

	siphash_transaction_hash_bytes[2..8].into()
}
