use std::collections::HashSet;
use rand::{thread_rng, Rng};
use bitcrypto::{sha256, siphash24};
use byteorder::{LittleEndian, ByteOrder};
use chain::{BlockHeader, ShortTransactionID, IndexedBlock};
use message::common::{BlockHeaderAndIDs, PrefilledTransaction};
use primitives::hash::H256;
use ser::{Stream, Serializable};

/// Maximum size of prefilled transactions in compact block
const MAX_COMPACT_BLOCK_PREFILLED_SIZE: usize = 10 * 1024;

pub fn build_compact_block(block: &IndexedBlock, prefilled_transactions_indexes: HashSet<usize>) -> BlockHeaderAndIDs {
	let nonce: u64 = thread_rng().gen();

	let prefilled_transactions_len = prefilled_transactions_indexes.len();
	let mut short_ids: Vec<ShortTransactionID> = Vec::with_capacity(block.transactions.len() - prefilled_transactions_len);
	let mut prefilled_transactions: Vec<PrefilledTransaction> = Vec::with_capacity(prefilled_transactions_len);
	let mut prefilled_transactions_size: usize = 0;

	let (key0, key1) = short_transaction_id_keys(nonce, &block.header.raw);
	for (transaction_index, transaction) in block.transactions.iter().enumerate() {
		let transaction_size = transaction.raw.serialized_size();
		if prefilled_transactions_size + transaction_size < MAX_COMPACT_BLOCK_PREFILLED_SIZE
			&& prefilled_transactions_indexes.contains(&transaction_index) {
			prefilled_transactions_size += transaction_size;
			prefilled_transactions.push(PrefilledTransaction {
				index: transaction_index,
				transaction: transaction.raw.clone(),
			})
		} else {
			short_ids.push(short_transaction_id(key0, key1, &transaction.hash));
		}
	}

	BlockHeaderAndIDs {
		header: block.header.raw.clone(),
		nonce: nonce,
		short_ids: short_ids,
		prefilled_transactions: prefilled_transactions,
	}
}

pub fn short_transaction_id_keys(nonce: u64, block_header: &BlockHeader) -> (u64, u64) {
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

	(key0, key1)
}

pub fn short_transaction_id(key0: u64, key1: u64, transaction_hash: &H256) -> ShortTransactionID {
	// 2) Running SipHash-2-4 with the input being the transaction ID and the keys (k0/k1) set to the first two little-endian
	// 64-bit integers from the above hash, respectively.
	let siphash_transaction_hash = siphash24(key0, key1, &**transaction_hash);

	// 3) Dropping the 2 most significant bytes from the SipHash output to make it 6 bytes.
	let mut siphash_transaction_hash_bytes = [0u8; 8];
	LittleEndian::write_u64(&mut siphash_transaction_hash_bytes, siphash_transaction_hash);

	siphash_transaction_hash_bytes[0..6].into()
}

#[cfg(test)]
mod tests {
	extern crate test_data;

	use std::collections::HashSet;
	use chain::{BlockHeader, Transaction, ShortTransactionID};
	use message::common::{BlockHeaderAndIDs, PrefilledTransaction};
	use super::*;

	#[test]
	fn short_transaction_id_is_correct() {
		// https://webbtc.com/tx/fa755807ab9f3ca8a9b25982570700f3f94bb0627f373893c3cfe79b5cf16def
		let transaction: Transaction = "01000000015fe01688dd8ae4428e21835c0e1b7af571c4223658d94da0c123e6fd7399862a010000006b483045022100f9e6d1bd3c9f54dcc72405994ec9ac2795878dd0b3cfbdc52bed28c2737fbecc02201fd68deab17bfaef1626e232cc4488dc273ba6fa5d807712b111d017cb96e0990121021fff64d1a21ede90d77cafa35fe7621db8aa433d947267980b395c35d23bd87fffffffff021ea56f72000000001976a9146fae1c8e7a648fff905dfdac9b019d3e887d7e8f88ac80f0fa02000000001976a9147f29b567c7dd9fc59cd3a7f716914966cc91ffa188ac00000000".into();
		let transaction_hash = transaction.hash();
		// https://webbtc.com/block/000000000000000001582cb2307ac43f3b4b268f2a75d3581d0babd48df1c300
		let block_header: BlockHeader = "000000205a54771c6a1a2bcc8f3412184f319dc02f7258b56fd5060100000000000000001de7a03cefe565d11cdfa369f6ffe59b9368a257203726c9cc363d31b4e3c2ebca4f3c58d4e6031830ccfd80".into();
		let nonce = 13450019974716797918_u64;
		let (key0, key1) = short_transaction_id_keys(nonce, &block_header);
		let actual_id = short_transaction_id(key0, key1, &transaction_hash);
		let expected_id: ShortTransactionID = "036e8b8b8f00".into();
		assert_eq!(expected_id, actual_id);
	}

	#[test]
	fn compact_block_is_built_correctly() {
		let block = test_data::block_builder().header().parent(test_data::genesis().hash()).build()
			.transaction().output().value(10).build().build()
			.transaction().output().value(20).build().build()
			.transaction().output().value(30).build().build()
			.build(); // genesis -> block
		let prefilled: HashSet<_> = vec![1].into_iter().collect();
		let compact_block = build_compact_block(&block.clone().into(), prefilled);
		let (key0, key1) = short_transaction_id_keys(compact_block.nonce, &block.block_header);
		let short_ids = vec![
			short_transaction_id(key0, key1, &block.transactions[0].hash()),
			short_transaction_id(key0, key1, &block.transactions[2].hash()),
		];
		assert_eq!(compact_block, BlockHeaderAndIDs {
			header: block.block_header.clone(),
			nonce: compact_block.nonce,
			short_ids: short_ids,
			prefilled_transactions: vec![
				PrefilledTransaction {
					index: 1,
					transaction: block.transactions[1].clone(),
				}
			],
		});
	}
}
