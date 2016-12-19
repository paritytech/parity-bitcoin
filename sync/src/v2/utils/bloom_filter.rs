use parking_lot::Mutex;
use bit_vec::BitVec;
use murmur3::murmur3_32;
use chain::{IndexedTransaction, OutPoint};
use message::types;
use ser::serialize;
use script::Script;

/// Constant optimized to create large differences in the seed for different values of `hash_functions_num`.
const SEED_OFFSET: u32 = 0xFBA4C795;

/// Connection bloom filter
#[derive(Debug)]
pub struct BloomFilter {
	/// Bloom data. Filter can be updated when transaction is matched => we have to use some kind of lock here.
	/// Mutex is an only choice, because:
	/// 1) we do not know if transaction matches the filter in advance
	/// 2) RwLock is non-upgradeable in Rust
	bloom: Option<Mutex<BloomFilterData>>,
	/// Filter update type.
	filter_flags: types::FilterFlags,
}

/// Bloom filter data implemented as described in:
/// https://github.com/bitcoin/bips/blob/master/bip-0037.mediawiki
#[derive(Debug, Default)]
struct BloomFilterData {
	/// Filter storage
	filter: BitVec,
	/// Number of hash functions to use in bloom filter
	hash_functions_num: u32,
	/// Value to add to Murmur3 hash seed when calculating hash
	tweak: u32,
}

impl Default for BloomFilter {
	fn default() -> Self {
		BloomFilter {
			bloom: None,
			filter_flags: types::FilterFlags::None,
		}
	}
}

impl BloomFilter {
	/// Create with given parameters
	#[cfg(test)]
	pub fn with_filter_load(message: types::FilterLoad) -> Self {
		BloomFilter {
			filter_flags: message.flags,
			bloom: Some(Mutex::new(BloomFilterData::with_filter_load(message))),
		}
	}

	/// Sets bloom filter to given value
	pub fn set_bloom_filter(&mut self, message: types::FilterLoad) {
		self.bloom = Some(Mutex::new(BloomFilterData::with_filter_load(message)));
	}

	/// Adds given data to current filter, so that new transactions can be accepted
	pub fn update_bloom_filter(&mut self, message: types::FilterAdd) {
		if let Some(ref mut bloom) = self.bloom {
			bloom.lock().insert(&message.data);
		}
	}

	/// Removes bloom filter, so that all transactions are now accepted by this filter
	pub fn remove_bloom_filter(&mut self) {
		self.bloom = None;
	}

	/// Filters transaction using bloom filter data
	pub fn filter_transaction(&self, tx: &IndexedTransaction) -> bool {
		// check with bloom filter, if set
		match self.bloom {
			/// if no filter is set for the connection => match everything
			None => true,
			/// filter using bloom filter, then update
			Some(ref bloom) => {
				let mut bloom = bloom.lock();
				let mut is_match = false;

				// match if filter contains any arbitrary script data element in any scriptPubKey in tx
				for (output_index, output) in tx.raw.outputs.iter().enumerate() {
					let script = Script::new(output.script_pubkey.clone());
					let is_update_needed = self.filter_flags == types::FilterFlags::All
						|| (self.filter_flags == types::FilterFlags::PubKeyOnly && (script.is_pay_to_public_key() || script.is_multisig_script()));
					for instruction in script.iter().filter_map(|i| i.ok()) {
						if let Some(instruction_data) = instruction.data {
							if bloom.contains(instruction_data) {
								is_match = true;

								if is_update_needed {
									bloom.insert(&serialize(&OutPoint {
										hash: tx.hash.clone(),
										index: output_index as u32,
									}));
								}
							}
						}
					}
				}

				// filter is updated only above => we can early-return from now
				if is_match {
					return is_match;
				}

				// match if filter contains transaction itself
				if bloom.contains(&*tx.hash) {
					return true;
				}

				// match if filter contains an outpoint this transaction spends
				for input in &tx.raw.inputs {
					// check if match previous output
					let previous_output = serialize(&input.previous_output);
					is_match = bloom.contains(&*previous_output);
					if is_match {
						return true;
					}

					// check if match any arbitrary script data element in any scriptSig in tx
					let script = Script::new(input.script_sig.clone());
					for instruction in script.iter().filter_map(|i| i.ok()) {
						if let Some(instruction_data) = instruction.data {
							is_match = bloom.contains(&*instruction_data);
							if is_match {
								return true;
							}
						}
					}
				}

				// no matches
				false
			},
		}
	}
}

impl BloomFilterData {
	/// Create with given parameters
	pub fn with_filter_load(message: types::FilterLoad) -> Self {
		BloomFilterData {
			filter: BitVec::from_bytes(&message.filter),
			hash_functions_num: message.hash_functions,
			tweak: message.tweak,
		}
	}

	/// True if filter contains given bytes
	pub fn contains(&self, data: &[u8]) -> bool {
		for hash_function_idx in 0..self.hash_functions_num {
			let murmur_seed = hash_function_idx.overflowing_mul(SEED_OFFSET).0.overflowing_add(self.tweak).0;
			let murmur_hash = murmur3_32(&mut data.as_ref(), murmur_seed) as usize % self.filter.len();
			if !self.filter.get(murmur_hash).expect("murmur_hash is result of mod operation by filter len; qed") {
				return false;
			}
		}
		true
	}

	/// Add bytes to the filter
	pub fn insert(&mut self, data: &[u8]) {
		for hash_function_idx in 0..self.hash_functions_num {
			let murmur_seed = hash_function_idx.overflowing_mul(SEED_OFFSET).0.overflowing_add(self.tweak).0;
			let murmur_hash = murmur3_32(&mut data.as_ref(), murmur_seed) as usize % self.filter.len();
			self.filter.set(murmur_hash, true);
		}
	}
}

#[cfg(test)]
pub mod tests {
	use std::iter::repeat;
	use chain::IndexedTransaction;
	use message::types;
	use primitives::bytes::Bytes;
	use primitives::hash::H256;
	use ser::serialize;
	use test_data;
	use super::{BloomFilter, BloomFilterData};

	fn default_filterload() -> types::FilterLoad {
		types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		}
	}

	fn make_filteradd(data: &[u8]) -> types::FilterAdd {
		types::FilterAdd {
			data: data.into(),
		}
	}

	#[test]
	fn bloom_insert_data() {
		let mut bloom = BloomFilterData::with_filter_load(default_filterload());

		assert!(!bloom.contains(&*H256::default()));

		bloom.insert(&*H256::default());
		assert!(bloom.contains(&*H256::default()));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_hash() {
		let tx1: IndexedTransaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: IndexedTransaction = test_data::TransactionBuilder::with_output(20).into();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&*tx1.hash));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_output_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// output script: OP_DUP OP_HASH160 380cb3c594de4e7e9b8e18db182987bebb5a4f70 OP_EQUALVERIFY OP_CHECKSIG
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_out_data: Bytes = "380cb3c594de4e7e9b8e18db182987bebb5a4f70".into();
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_out_data));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn bloom_filter_matches_transaction_by_previous_output_point() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_previous_output: Bytes = serialize(&tx1.raw.inputs[0].previous_output);
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_previous_output));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}

	#[test]
	fn connection_filter_matches_transaction_by_input_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// input script: PUSH DATA 304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01
		let tx1: IndexedTransaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_input_data: Bytes = "304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01".into();
		let tx2 = IndexedTransaction::default();

		let mut filter = BloomFilter::with_filter_load(default_filterload());

		assert!(!filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));

		filter.update_bloom_filter(make_filteradd(&tx1_input_data));

		assert!(filter.filter_transaction(&tx1));
		assert!(!filter.filter_transaction(&tx2));
	}
}
