#![allow(dead_code)]

use bit_vec::BitVec;
use murmur3::murmur3_32;
use chain::{Transaction, OutPoint};
use ser::serialize;
use message::types;
use script::Script;

/// Constant optimized to create large differences in the seed for different values of hash_functions_num.
const SEED_OFFSET: u32 = 0xFBA4C795;

/// Filter, which controls data relayed over connection.
#[derive(Debug, Default)]
pub struct ConnectionFilter {
	/// Bloom filter, if set.
	bloom: Option<ConnectionBloom>,
	/// Filter update type.
	filter_flags: u8,
}

/// Connection bloom filter
#[derive(Debug)]
struct ConnectionBloom {
	/// Filter storage.
	filter: BitVec,
	/// Number of hash functions to use in bloom filter.
	hash_functions_num: u32,
	/// Value to add to Murmur3 hash seed when calculating hash.
	tweak: u32,
}

impl ConnectionFilter {
	#[cfg(test)]
	/// Create new connection with given filter params
	pub fn with_filterload(message: &types::FilterLoad) -> Self {
		ConnectionFilter {
			bloom: Some(ConnectionBloom::new(message)),
			filter_flags: message.flags,
		}
	}

	/// Check if transaction is matched && update filter
	pub fn match_update_transaction(&mut self, transaction: &Transaction) -> bool {
		match self.bloom {
			/// if no filter is set for the connection => match everything
			None => true,
			/// filter using bloom filter, then update
			Some(ref mut bloom) => {
				let transaction_hash = transaction.hash();
				let mut is_match = false;

				// match if filter contains any arbitrary script data element in any scriptPubKey in tx
				for (output_index, output) in transaction.outputs.iter().enumerate() {
					let script = Script::new(output.script_pubkey.clone());
					for instruction in script.iter().filter_map(|i| i.ok()) {
						if let Some(instruction_data) = instruction.data {
							if bloom.contains(instruction_data) {
								is_match = true;

								let is_update_needed = self.filter_flags == 1
									|| (self.filter_flags == 2 && (script.is_pay_to_public_key() || script.is_multisig_script()));
								if is_update_needed {
									bloom.insert(&serialize(&OutPoint {
										hash: transaction_hash.clone(),
										index: output_index as u32,
									}));
								}
							}
						}
					}
				}
				if is_match {
					return is_match;
				}

				// match if filter contains transaction itself
				if bloom.contains(&*transaction_hash) {
					return true;
				}

				// match if filter contains an outpoint this transaction spends
				for input in &transaction.inputs {
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

	/// Load filter
	pub fn load(&mut self, message: &types::FilterLoad) {
		self.bloom = Some(ConnectionBloom::new(message));
		self.filter_flags = message.flags;
	}

	/// Add filter
	pub fn add(&mut self, message: &types::FilterAdd) {
		// ignore if filter is not currently set
		if let Some(ref mut bloom) = self.bloom {
			bloom.insert(&message.data);
		}
	}

	/// Clear filter
	pub fn clear(&mut self) {
		self.bloom = None;
	}
}

impl ConnectionBloom {
	/// Create with given parameters
	pub fn new(message: &types::FilterLoad) -> Self {
		ConnectionBloom {
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
			if !self.filter.get(murmur_hash).expect("mod operation above") {
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
mod tests {
	use std::iter::{Iterator, repeat};
	use test_data;
	use message::types;
	use chain::{Transaction, RepresentH256};
	use primitives::hash::H256;
	use primitives::bytes::Bytes;
	use super::{ConnectionFilter, ConnectionBloom};

	fn default_filterload() -> types::FilterLoad {
		types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: 0,
		}
	}

	fn make_filteradd(data: &[u8]) -> types::FilterAdd {
		types::FilterAdd {
			data: data.into(),
		}
	}

	#[test]
	fn bloom_insert_data() {
		let mut bloom = ConnectionBloom::new(&default_filterload());

		assert!(!bloom.contains(&*H256::default()));

		bloom.insert(&*H256::default());
		assert!(bloom.contains(&*H256::default()));
	}

	#[test]
	fn connection_filter_matches_transaction_by_hash() {
		let tx1: Transaction = test_data::TransactionBuilder::with_output(10).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_output(20).into();

		let mut filter = ConnectionFilter::with_filterload(&default_filterload());

		assert!(!filter.match_update_transaction(&tx1));
		assert!(!filter.match_update_transaction(&tx2));

		filter.add(&make_filteradd(&*tx1.hash()));

		assert!(filter.match_update_transaction(&tx1));
		assert!(!filter.match_update_transaction(&tx2));
	}

	#[test]
	fn connection_filter_matches_transaction_by_output_script_data_element() {
		// TODO
	}

	#[test]
	fn connection_filter_matches_transaction_by_previous_output_point() {
		// TODO
	}

	#[test]
	fn connection_filter_matches_transaction_by_input_script_data_element() {
		// TODO
	}
}