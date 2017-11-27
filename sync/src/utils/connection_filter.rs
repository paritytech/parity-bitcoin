use bit_vec::BitVec;
use chain::{IndexedBlock, IndexedTransaction};
use message::types;
use primitives::bytes::Bytes;
use primitives::hash::H256;
use synchronization_peers::MerkleBlockArtefacts;
use utils::{KnownHashFilter, KnownHashType, BloomFilter, FeeRateFilter, build_compact_block, build_partial_merkle_tree};

/// Filter, which controls data relayed over connection.
#[derive(Debug, Default)]
pub struct ConnectionFilter {
	/// Known hashes filter
	known_hash_filter: KnownHashFilter,
	/// Fee rate filter
	fee_rate_filter: FeeRateFilter,
	/// Bloom filter
	bloom_filter: BloomFilter,
}

impl ConnectionFilter {
	/// Add known item hash
	pub fn hash_known_as(&mut self, hash: H256, hash_type: KnownHashType) {
		self.known_hash_filter.insert(hash, hash_type);
	}

	/// Is item with given hash && type is known by peer
	pub fn is_hash_known_as(&self, hash: &H256, hash_type: KnownHashType) -> bool {
		self.known_hash_filter.contains(hash, hash_type)
	}

	/// Check if block should be sent to this connection
	pub fn filter_block(&self, block_hash: &H256) -> bool {
		self.known_hash_filter.filter_block(block_hash)
	}

	/// Check if transaction should be sent to this connection && optionally update filter
	pub fn filter_transaction(&self, transaction: &IndexedTransaction, transaction_fee_rate: Option<u64>) -> bool {
		self.known_hash_filter.filter_transaction(&transaction.hash)
			&& self.fee_rate_filter.filter_transaction(transaction_fee_rate)
			&& self.bloom_filter.filter_transaction(transaction)
	}

	/// Load filter
	pub fn load(&mut self, message: types::FilterLoad) {
		self.bloom_filter.set_bloom_filter(message);
	}

	/// Add filter
	pub fn add(&mut self, message: types::FilterAdd) {
		self.bloom_filter.update_bloom_filter(message);
	}

	/// Clear filter
	pub fn clear(&mut self) {
		self.bloom_filter.remove_bloom_filter();
	}

	/// Limit transaction announcing by transaction fee
	pub fn set_fee_rate(&mut self, message: types::FeeFilter) {
		self.fee_rate_filter.set_min_fee_rate(message);
	}

	/// Convert block to compact block using this filter
	pub fn build_compact_block(&self, block: &IndexedBlock) -> types::CompactBlock {
		let unknown_transaction_indexes = block.transactions.iter().enumerate()
			.filter(|&(_, tx)| self.known_hash_filter.contains(&tx.hash, KnownHashType::Transaction))
			.map(|(idx, _)| idx)
			.collect();
		types::CompactBlock {
			header: build_compact_block(block, unknown_transaction_indexes),
		}
	}

	/// Convert `Block` to `MerkleBlock` using this filter
	pub fn build_merkle_block(&self, block: &IndexedBlock) -> Option<MerkleBlockArtefacts> {
		if !self.bloom_filter.is_set() {
			// only respond when bloom filter is set
			return None;
		}

		// prepare result
		let all_len = block.transactions.len();
		let mut result = MerkleBlockArtefacts {
			merkleblock: types::MerkleBlock {
				block_header: block.header.raw.clone(),
				total_transactions: all_len as u32,
				hashes: Vec::default(),
				flags: Bytes::default(),
			},
			matching_transactions: Vec::new(),
		};

		// calculate hashes && match flags for all transactions
		let (all_hashes, all_flags) = block.transactions.iter()
			.fold((Vec::<H256>::with_capacity(all_len), BitVec::with_capacity(all_len)), |(mut all_hashes, mut all_flags), t| {
				let flag = self.bloom_filter.filter_transaction(t);
				all_flags.push(flag);
				all_hashes.push(t.hash.clone());
				if flag {
					result.matching_transactions.push(t.clone());
				}
				(all_hashes, all_flags)
			});

		// build partial merkle tree
		let partial_merkle_tree = build_partial_merkle_tree(all_hashes, all_flags);
		result.merkleblock.hashes.extend(partial_merkle_tree.hashes);
		// to_bytes() converts [true, false, true] to 0b10100000
		// while protocol requires [true, false, true] to be serialized as 0x00000101
		result.merkleblock.flags = partial_merkle_tree.flags.to_bytes().into_iter()
			.map(|b|
				((b & 0b10000000) >> 7) |
				((b & 0b01000000) >> 5) |
				((b & 0b00100000) >> 3) |
				((b & 0b00010000) >> 1) |
				((b & 0b00001000) << 1) |
				((b & 0b00000100) << 3) |
				((b & 0b00000010) << 5) |
				((b & 0b00000001) << 7)).collect::<Vec<u8>>().into();
		Some(result)
	}
}

#[cfg(test)]
pub mod tests {
	extern crate test_data;

	use std::iter::repeat;
	use chain::IndexedTransaction;
	use message::types;
	use primitives::bytes::Bytes;
	use super::ConnectionFilter;
	use utils::KnownHashType;

	#[test]
	fn filter_default_accepts_block() {
		assert!(ConnectionFilter::default().filter_block(&test_data::genesis().hash()));
	}

	#[test]
	fn filter_default_accepts_transaction() {
		assert!(ConnectionFilter::default().filter_transaction(&test_data::genesis().transactions[0].clone().into(), Some(0)));
	}

	#[test]
	fn filter_rejects_block_known() {
		let mut filter = ConnectionFilter::default();
		filter.hash_known_as(test_data::block_h1().hash(), KnownHashType::Block);
		filter.hash_known_as(test_data::block_h2().hash(), KnownHashType::CompactBlock);
		assert!(!filter.filter_block(&test_data::block_h1().hash()));
		assert!(!filter.filter_block(&test_data::block_h2().hash()));
		assert!(filter.filter_block(&test_data::genesis().hash()));
	}

	#[test]
	fn filter_rejects_transaction_known() {
		let mut filter = ConnectionFilter::default();
		filter.hash_known_as(test_data::block_h1().transactions[0].hash(), KnownHashType::Transaction);
		assert!(!filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), None));
		assert!(filter.filter_transaction(&test_data::block_h2().transactions[0].clone().into(), None));
	}

	#[test]
	fn filter_rejects_transaction_feerate() {
		let mut filter = ConnectionFilter::default();
		filter.set_fee_rate(types::FeeFilter::with_fee_rate(1000));
		assert!(filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), None));
		assert!(filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), Some(1500)));
		assert!(!filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), Some(500)));
	}

	#[test]
	fn filter_rejects_transaction_bloomfilter() {
		let mut filter = ConnectionFilter::default();
		let tx: IndexedTransaction = test_data::block_h1().transactions[0].clone().into();
		filter.load(types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		});
		assert!(!filter.filter_transaction(&tx, None));
		filter.add(types::FilterAdd {
			data: (&*tx.hash as &[u8]).into(),
		});
		assert!(filter.filter_transaction(&tx, None));
		filter.clear();
		assert!(filter.filter_transaction(&tx, None));
	}
}
