use chain::{IndexedBlock, IndexedTransaction};
use message::types;
use primitives::hash::H256;
use super::utils::{KnownHashType, KnownHashFilter, FeeRateFilter, BloomFilter};

/// Synchronization peer filter
pub trait Filter {
	/// Check if block is known by the peer
	fn filter_block(&self, block: &IndexedBlock) -> bool;
	/// Check if transaction is known by the peer
	fn filter_transaction(&self, transaction: &IndexedTransaction, fee_rate: Option<u64>) -> bool;
	/// Remember that peer knows given block
	fn remember_known_block(&mut self, hash: H256);
	/// Remember that peer knows given compact block
	fn remember_known_compact_block(&mut self, hash: H256);
	/// Remember that peer knows given transaction
	fn remember_known_transaction(&mut self, hash: H256);
	/// Set up bloom filter
	fn set_bloom_filter(&mut self, filter: types::FilterLoad);
	/// Update bloom filter
	fn update_bloom_filter(&mut self, filter: types::FilterAdd);
	/// Remove bloom filter
	fn remove_bloom_filter(&mut self);
	/// Set up fee rate filter
	fn set_min_fee_rate(&mut self, filter: types::FeeFilter);
}

/// Synchronization peer filter implementation
#[derive(Default)]
pub struct FilterImpl {
	/// Known hashes filter
	known_hash_filter: KnownHashFilter,
	/// Feerate filter
	fee_rate_filter: FeeRateFilter,
	/// Bloom filter
	bloom_filter: BloomFilter,
}

impl Filter for FilterImpl {
	fn filter_block(&self, block: &IndexedBlock) -> bool {
		self.known_hash_filter.filter_block(&block.header.hash)
	}

	fn filter_transaction(&self, transaction: &IndexedTransaction, fee_rate: Option<u64>) -> bool {
		self.known_hash_filter.filter_transaction(&transaction.hash)
			&& fee_rate.map(|fee_rate| self.fee_rate_filter.filter_transaction(fee_rate)).unwrap_or(false)
			&& self.bloom_filter.filter_transaction(transaction)
	}

	fn remember_known_block(&mut self, hash: H256) {
		self.known_hash_filter.insert(hash, KnownHashType::Block);
	}

	fn remember_known_compact_block(&mut self, hash: H256) {
		self.known_hash_filter.insert(hash, KnownHashType::CompactBlock);
	}

	fn remember_known_transaction(&mut self, hash: H256) {
		self.known_hash_filter.insert(hash, KnownHashType::Transaction);
	}

	fn set_bloom_filter(&mut self, filter: types::FilterLoad) {
		self.bloom_filter.set_bloom_filter(filter);
	}

	fn update_bloom_filter(&mut self, filter: types::FilterAdd) {
		self.bloom_filter.update_bloom_filter(filter);
	}

	fn remove_bloom_filter(&mut self) {
		self.bloom_filter.remove_bloom_filter();
	}

	fn set_min_fee_rate(&mut self, filter: types::FeeFilter) {
		self.fee_rate_filter.set_min_fee_rate(filter);
	}
}

#[cfg(test)]
mod tests {
	use std::iter::repeat;
	use chain::IndexedTransaction;
	use message::types;
	use primitives::bytes::Bytes;
	use test_data;
	use super::{Filter, FilterImpl};

	#[test]
	fn filter_default_accepts_block() {
		assert!(FilterImpl::default().filter_block(&test_data::genesis().into()));
	}

	#[test]
	fn filter_default_accepts_transaction() {
		assert!(FilterImpl::default().filter_transaction(&test_data::genesis().transactions[0].clone().into(), Some(0)));
	}

	#[test]
	fn filter_rejects_block_known() {
		let mut filter = FilterImpl::default();
		filter.remember_known_block(test_data::block_h1().hash());
		filter.remember_known_compact_block(test_data::block_h2().hash());
		assert!(!filter.filter_block(&test_data::block_h1().into()));
		assert!(!filter.filter_block(&test_data::block_h2().into()));
		assert!(filter.filter_block(&test_data::genesis().into()));
	}

	#[test]
	fn filter_rejects_transaction_known() {
		let mut filter = FilterImpl::default();
		filter.remember_known_transaction(test_data::block_h1().transactions[0].hash());
		assert!(!filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), None));
		assert!(!filter.filter_transaction(&test_data::block_h2().transactions[0].clone().into(), None));
	}

	#[test]
	fn filter_rejects_transaction_feerate() {
		let mut filter = FilterImpl::default();
		filter.set_min_fee_rate(types::FeeFilter::with_fee_rate(1000));
		assert!(filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), None));
		assert!(filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), Some(1500)));
		assert!(!filter.filter_transaction(&test_data::block_h1().transactions[0].clone().into(), Some(500)));
	}

	#[test]
	fn filter_rejects_transaction_bloomfilter() {
		let mut filter = FilterImpl::default();
		let tx: IndexedTransaction = test_data::block_h1().transactions[0].clone().into();
		filter.set_bloom_filter(types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		});
		assert!(!filter.filter_transaction(&tx, None));
		filter.update_bloom_filter(types::FilterAdd {
			data: (&*tx.hash as &[u8]).into(),
		});
		assert!(filter.filter_transaction(&tx, None));
		filter.remove_bloom_filter();
		assert!(filter.filter_transaction(&tx, None));
	}
}
