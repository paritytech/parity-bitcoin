use std::cmp::min;
use linked_hash_map::LinkedHashMap;
use bit_vec::BitVec;
use murmur3::murmur3_32;
use chain::{Block, Transaction, OutPoint, merkle_node_hash};
use ser::serialize;
use message::types;
use primitives::bytes::Bytes;
use primitives::hash::H256;
use script::Script;

/// Constant optimized to create large differences in the seed for different values of `hash_functions_num`.
const SEED_OFFSET: u32 = 0xFBA4C795;
/// Max last blocks to store for given peer. TODO: how bitcoind deals with this?
pub const MAX_LAST_BLOCKS_TO_STORE: usize = 2048;
/// Max last transactions to store for given peer
pub const MAX_LAST_TRANSACTIONS_TO_STORE: usize = 64;

/// Filter, which controls data relayed over connection.
#[derive(Debug)]
pub struct ConnectionFilter {
	/// Bloom filter, if set.
	bloom: Option<ConnectionBloom>,
	/// Filter update type.
	filter_flags: types::FilterFlags,
	/// Last blocks from peer.
	last_blocks: LinkedHashMap<H256, ()>,
	/// Last transactions from peer.
	last_transactions: LinkedHashMap<H256, ()>,
	/// Minimal fee in satoshis per 1000 bytes
	fee_rate: Option<u64>,
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

/// `merkleblock` build artefacts
#[derive(Debug, PartialEq)]
pub struct MerkleBlockArtefacts {
	/// `merkleblock` message
	pub merkleblock: types::MerkleBlock,
	/// All matching transactions
	pub matching_transactions: Vec<(H256, Transaction)>,
}

/// Service structure to construct `merkleblock` message.
struct PartialMerkleTree {
	/// All transactions length.
	all_len: usize,
	/// All transactions hashes.
	all_hashes: Vec<H256>,
	/// Match flags for all transactions.
	all_matches: BitVec,
	/// Partial hashes.
	hashes: Vec<H256>,
	/// Partial match flags.
	matches: BitVec,
}

impl Default for ConnectionFilter {
	fn default() -> Self {
		ConnectionFilter {
			bloom: None,
			filter_flags: types::FilterFlags::None,
			last_blocks: LinkedHashMap::new(),
			last_transactions: LinkedHashMap::new(),
			fee_rate: None,
		}
	}
}

impl ConnectionFilter {
	#[cfg(test)]
	/// Create new connection with given filter params
	pub fn with_filterload(message: &types::FilterLoad) -> Self {
		ConnectionFilter {
			bloom: Some(ConnectionBloom::new(message)),
			filter_flags: message.flags,
			last_blocks: LinkedHashMap::new(),
			last_transactions: LinkedHashMap::new(),
			fee_rate: None,
		}
	}

	/// We have a knowledge that block with given hash is known to this connection
	pub fn known_block(&mut self, block_hash: &H256) {
		// remember that peer knows about this block
		if !self.last_blocks.contains_key(block_hash) {
			if self.last_blocks.len() == MAX_LAST_BLOCKS_TO_STORE {
				self.last_blocks.pop_front();
			}

			self.last_blocks.insert(block_hash.clone(), ());
		}
	}

	/// We have a knowledge that transaction with given hash is known to this connection
	pub fn known_transaction(&mut self, transaction_hash: &H256) {
		// remember that peer knows about this block
		if !self.last_transactions.contains_key(transaction_hash) {
			if self.last_transactions.len() == MAX_LAST_TRANSACTIONS_TO_STORE {
				self.last_transactions.pop_front();
			}

			self.last_transactions.insert(transaction_hash.clone(), ());
		}
	}

	/// Check if block should be sent to this connection
	pub fn filter_block(&self, block_hash: &H256) -> bool {
		// check if block is known
		!self.last_blocks.contains_key(block_hash)
	}

	/// Check if transaction should be sent to this connection && optionally update filter
	pub fn filter_transaction(&mut self, transaction_hash: &H256, transaction: &Transaction, transaction_fee_rate: Option<u64>) -> bool {
		// check if transaction is known
		if self.last_transactions.contains_key(transaction_hash) {
			return false;
		}

		// check if transaction fee rate is high enough for this peer
		if let Some(fee_rate) = self.fee_rate {
			if let Some(transaction_fee_rate) = transaction_fee_rate {
				if transaction_fee_rate < fee_rate {
					return false;
				}
			}
		}

		// check with bloom filter, if set
		self.filter_transaction_with_bloom(transaction_hash, transaction)
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

	/// Limit transaction announcing by transaction fee
	pub fn set_fee_rate(&mut self, fee_rate: u64) {
		if fee_rate == 0 {
			self.fee_rate = None;
		}
		else {
			self.fee_rate = Some(fee_rate);
		}
	}

	/// Convert `Block` to `MerkleBlock` using this filter
	pub fn build_merkle_block(&mut self, block: Block) -> Option<MerkleBlockArtefacts> {
		if self.bloom.is_none() {
			return None;
		}

		// prepare result
		let all_len = block.transactions.len();
		let mut result = MerkleBlockArtefacts {
			merkleblock: types::MerkleBlock {
				block_header: block.block_header.clone(),
				total_transactions: all_len as u32,
				hashes: Vec::default(),
				flags: Bytes::default(),
			},
			matching_transactions: Vec::new(),
		};

		// calculate hashes && match flags for all transactions
		let (all_hashes, all_flags) = block.transactions.into_iter()
			.fold((Vec::<H256>::with_capacity(all_len), BitVec::with_capacity(all_len)), |(mut all_hashes, mut all_flags), t| {
				let hash = t.hash();
				let flag = self.filter_transaction_with_bloom(&hash, &t);
				if flag {
					result.matching_transactions.push((hash.clone(), t));
				}

				all_flags.push(flag);
				all_hashes.push(hash);
				(all_hashes, all_flags)
			});

		// build partial merkle tree
		let (hashes, flags) = PartialMerkleTree::build(all_hashes, all_flags);
		result.merkleblock.hashes.extend(hashes);
		// to_bytes() converts [true, false, true] to 0b10100000
		// while protocol requires [true, false, true] to be serialized as 0x00000101
		result.merkleblock.flags = flags.to_bytes().into_iter()
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

	/// Check if transaction should be sent to this connection using bloom filter && optionally update filter
	fn filter_transaction_with_bloom(&mut self, transaction_hash: &H256, transaction: &Transaction) -> bool {
		// check with bloom filter, if set
		match self.bloom {
			/// if no filter is set for the connection => match everything
			None => true,
			/// filter using bloom filter, then update
			Some(ref mut bloom) => {
				let mut is_match = false;

				// match if filter contains any arbitrary script data element in any scriptPubKey in tx
				for (output_index, output) in transaction.outputs.iter().enumerate() {
					let script = Script::new(output.script_pubkey.clone());
					for instruction in script.iter().filter_map(|i| i.ok()) {
						if let Some(instruction_data) = instruction.data {
							if bloom.contains(instruction_data) {
								is_match = true;

								let is_update_needed = self.filter_flags == types::FilterFlags::All
									|| (self.filter_flags == types::FilterFlags::PubKeyOnly && (script.is_pay_to_public_key() || script.is_multisig_script()));
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
				if bloom.contains(&**transaction_hash) {
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

impl PartialMerkleTree {
	/// Build partial merkle tree as described here:
	/// https://bitcoin.org/en/developer-reference#creating-a-merkleblock-message
	pub fn build(all_hashes: Vec<H256>, all_matches: BitVec) -> (Vec<H256>, BitVec) {
		let mut partial_merkle_tree = PartialMerkleTree {
			all_len: all_hashes.len(),
			all_hashes: all_hashes,
			all_matches: all_matches,
			hashes: Vec::new(),
			matches: BitVec::new(),
		};
		partial_merkle_tree.build_tree();
		(partial_merkle_tree.hashes, partial_merkle_tree.matches)
	}

	#[cfg(test)]
	/// Parse partial merkle tree as described here:
	/// https://bitcoin.org/en/developer-reference#parsing-a-merkleblock-message
	pub fn parse(all_len: usize, hashes: Vec<H256>, matches: BitVec) -> Result<(H256, Vec<H256>, BitVec), String> {
		let mut partial_merkle_tree = PartialMerkleTree {
			all_len: all_len,
			all_hashes: Vec::new(),
			all_matches: BitVec::from_elem(all_len, false),
			hashes: hashes,
			matches: matches,
		};

		let merkle_root = try!(partial_merkle_tree.parse_tree());
		Ok((merkle_root, partial_merkle_tree.all_hashes, partial_merkle_tree.all_matches))
	}

	fn build_tree(&mut self) {
		let tree_height = self.tree_height();
		self.build_branch(tree_height, 0)
	}

	#[cfg(test)]
	fn parse_tree(&mut self) -> Result<H256, String> {
		if self.all_len == 0 {
			return Err("no transactions".into());
		}
		if self.hashes.len() > self.all_len {
			return Err("too many hashes".into());
		}
		if self.matches.len() < self.hashes.len() {
			return Err("too few matches".into());
		}

		// parse tree
		let mut matches_used = 0usize;
		let mut hashes_used = 0usize;
		let tree_height = self.tree_height();
		let merkle_root = try!(self.parse_branch(tree_height, 0, &mut matches_used, &mut hashes_used));

		if matches_used != self.matches.len() {
			return Err("not all matches used".into());
		}
		if hashes_used != self.hashes.len() {
			return Err("not all hashes used".into());
		}

		Ok(merkle_root)
	}

	fn build_branch(&mut self, height: usize, pos: usize) {
		// determine whether this node is the parent of at least one matched txid
		let transactions_begin = pos << height;
		let transactions_end = min(self.all_len, (pos + 1) << height);
		let flag = (transactions_begin..transactions_end).any(|idx| self.all_matches[idx]);
		// remember flag
		self.matches.push(flag);
		// proceeed with descendants
		if height == 0 || !flag {
			// we're at the leaf level || there is no match
			let hash = self.branch_hash(height, pos);
			self.hashes.push(hash);
		} else {
			// proceed with left child
			self.build_branch(height - 1, pos << 1);
			// proceed with right child if any
			if (pos << 1) + 1 < self.level_width(height - 1) {
				self.build_branch(height - 1, (pos << 1) + 1);
			}
		}
	}

	#[cfg(test)]
	fn parse_branch(&mut self, height: usize, pos: usize, matches_used: &mut usize, hashes_used: &mut usize) -> Result<H256, String> {
		if *matches_used >= self.matches.len() {
			return Err("all matches used".into());
		}

		let flag = self.matches[*matches_used];
		*matches_used += 1;

		if height == 0 || !flag {
			// we're at the leaf level || there is no match
			if *hashes_used > self.hashes.len() {
				return Err("all hashes used".into());
			}

			// get node hash
			let ref hash = self.hashes[*hashes_used];
			*hashes_used += 1;

			// on leaf level && matched flag set => mark transaction as matched
			if height == 0 && flag {
				self.all_hashes.push(hash.clone());
				self.all_matches.set(pos, true);
			}

			Ok(hash.clone())
		} else {
			// proceed with left child
			let left = try!(self.parse_branch(height - 1, pos << 1, matches_used, hashes_used));
			// proceed with right child if any
			let has_right_child = (pos << 1) + 1 < self.level_width(height - 1);
			let right = if has_right_child {
				try!(self.parse_branch(height - 1, (pos << 1) + 1, matches_used, hashes_used))
			} else {
				left.clone()
			};

			if has_right_child && left == right {
				Err("met same hash twice".into())
			} else {
				Ok(merkle_node_hash(&left, &right))
			}
		}
	}

	fn tree_height(&self) -> usize {
		let mut height = 0usize;
		while self.level_width(height) > 1 {
			height += 1;
		}
		height
	}

	fn level_width(&self, height: usize) -> usize {
		(self.all_len + (1 << height) - 1) >> height
	}

	fn branch_hash(&self, height: usize, pos: usize) -> H256 {
		if height == 0 {
			self.all_hashes[pos].clone()
		} else {
			let left = self.branch_hash(height - 1, pos << 1);
			let right = if (pos << 1) + 1 < self.level_width(height - 1) {
				self.branch_hash(height - 1, (pos << 1) + 1)
			} else {
				left.clone()
			};

			merkle_node_hash(&left, &right)
		}
	}
}

#[cfg(test)]
pub mod tests {
	use std::iter::{Iterator, repeat};
	use test_data;
	use message::types;
	use chain::{merkle_root, Transaction};
	use primitives::hash::H256;
	use primitives::bytes::Bytes;
	use ser::serialize;
	use super::{ConnectionFilter, ConnectionBloom, PartialMerkleTree,
		MAX_LAST_BLOCKS_TO_STORE, MAX_LAST_TRANSACTIONS_TO_STORE};

	pub fn default_filterload() -> types::FilterLoad {
		types::FilterLoad {
			filter: Bytes::from(repeat(0u8).take(1024).collect::<Vec<_>>()),
			hash_functions: 10,
			tweak: 5,
			flags: types::FilterFlags::None,
		}
	}

	pub fn make_filteradd(data: &[u8]) -> types::FilterAdd {
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

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));

		filter.add(&make_filteradd(&*tx1.hash()));

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));
	}

	#[test]
	fn connection_filter_matches_transaction_by_output_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// output script: OP_DUP OP_HASH160 380cb3c594de4e7e9b8e18db182987bebb5a4f70 OP_EQUALVERIFY OP_CHECKSIG
		let tx1: Transaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_out_data: Bytes = "380cb3c594de4e7e9b8e18db182987bebb5a4f70".into();
		let tx2 = Transaction::default();

		let mut filter = ConnectionFilter::with_filterload(&default_filterload());

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));

		filter.add(&make_filteradd(&tx1_out_data));

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));
	}

	#[test]
	fn connection_filter_matches_transaction_by_previous_output_point() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		let tx1: Transaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_previous_output: Bytes = serialize(&tx1.inputs[0].previous_output);
		let tx2 = Transaction::default();

		let mut filter = ConnectionFilter::with_filterload(&default_filterload());

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));

		filter.add(&make_filteradd(&tx1_previous_output));

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));
	}

	#[test]
	fn connection_filter_matches_transaction_by_input_script_data_element() {
		// https://webbtc.com/tx/eb3b82c0884e3efa6d8b0be55b4915eb20be124c9766245bcc7f34fdac32bccb
		// input script: PUSH DATA 304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01
		let tx1: Transaction = "01000000024de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8000000006b48304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b0121035aa98d5f77cd9a2d88710e6fc66212aff820026f0dad8f32d1f7ce87457dde50ffffffff4de8b0c4c2582db95fa6b3567a989b664484c7ad6672c85a3da413773e63fdb8010000006f004730440220276d6dad3defa37b5f81add3992d510d2f44a317fd85e04f93a1e2daea64660202200f862a0da684249322ceb8ed842fb8c859c0cb94c81e1c5308b4868157a428ee01ab51210232abdc893e7f0631364d7fd01cb33d24da45329a00357b3a7886211ab414d55a51aeffffffff02e0fd1c00000000001976a914380cb3c594de4e7e9b8e18db182987bebb5a4f7088acc0c62d000000000017142a9bc5447d664c1d0141392a842d23dba45c4f13b17500000000".into();
		let tx1_input_data: Bytes = "304502205b282fbc9b064f3bc823a23edcc0048cbb174754e7aa742e3c9f483ebe02911c022100e4b0b3a117d36cab5a67404dddbf43db7bea3c1530e0fe128ebc15621bd69a3b01".into();
		let tx2 = Transaction::default();

		let mut filter = ConnectionFilter::with_filterload(&default_filterload());

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));

		filter.add(&make_filteradd(&tx1_input_data));

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, None));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, None));
	}

	#[test]
	fn connection_filter_matches_transaction_by_fee_rate() {
		let tx1: Transaction = test_data::TransactionBuilder::with_version(1).into();
		let tx2: Transaction = test_data::TransactionBuilder::with_version(2).into();

		let mut filter = ConnectionFilter::default();

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, Some(1000)));
		assert!(filter.filter_transaction(&tx2.hash(), &tx2, Some(2000)));

		filter.set_fee_rate(1500);

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, Some(1000)));
		assert!(filter.filter_transaction(&tx2.hash(), &tx2, Some(2000)));

		filter.set_fee_rate(3000);

		assert!(!filter.filter_transaction(&tx1.hash(), &tx1, Some(1000)));
		assert!(!filter.filter_transaction(&tx2.hash(), &tx2, Some(2000)));

		filter.set_fee_rate(0);

		assert!(filter.filter_transaction(&tx1.hash(), &tx1, Some(1000)));
		assert!(filter.filter_transaction(&tx2.hash(), &tx2, Some(2000)));
	}

	#[test]
	// test from core implementation (slow)
	// https://github.com/bitcoin/bitcoin/blob/master/src/test/pmt_tests.cpp
	fn test_build_merkle_block() {
		use bit_vec::BitVec;
		use rand::{Rng, SeedableRng, StdRng};

		let rng_seed: &[_] = &[0, 0, 0, 0];
		let mut rng: StdRng = SeedableRng::from_seed(rng_seed);

		// for some transactions counts
		let tx_counts: Vec<usize> = vec![1, 4, 7, 17, 56, 100, 127, 256, 312, 513, 1000, 4095];
		for tx_count in tx_counts {
			// build block with given transactions number
			let transactions: Vec<Transaction> = (0..tx_count).map(|n| test_data::TransactionBuilder::with_version(n as i32).into()).collect();
			let hashes: Vec<_> = transactions.iter().map(|t| t.hash()).collect();
			let merkle_root = merkle_root(&hashes);

			// mark different transactions as matched
			for seed_tweak in 1..15 {
				let mut matches: BitVec = BitVec::with_capacity(tx_count);
				let mut matched_hashes: Vec<H256> = Vec::with_capacity(tx_count);
				for i in 0usize..tx_count {
					let is_match = (rng.gen::<u32>() & ((1 << (seed_tweak / 2)) - 1)) == 0;
					matches.push(is_match);
					if is_match {
						matched_hashes.push(hashes[i].clone());
					}
				}

				// build partial merkle tree
				let (built_hashes, built_flags) = PartialMerkleTree::build(hashes.clone(), matches.clone());
				// parse tree back
				let (parsed_root, parsed_hashes, parsed_positions) = PartialMerkleTree::parse(tx_count, built_hashes, built_flags)
					.expect("no error");

				assert_eq!(matched_hashes, parsed_hashes);
				assert_eq!(matches, parsed_positions);
				assert_eq!(merkle_root, parsed_root);
			}
		}
	}

	#[test]
	fn block_is_filtered_out_when_it_is_received_from_peer() {
		let blocks = test_data::build_n_empty_blocks_from_genesis((MAX_LAST_BLOCKS_TO_STORE + 1) as u32, 1);

		let mut filter = ConnectionFilter::default();
		assert!(filter.filter_block(&blocks[0].hash()));

		filter.known_block(&blocks[0].hash());
		assert!(!filter.filter_block(&blocks[0].hash()));

		for block in blocks.iter().skip(1).take(MAX_LAST_BLOCKS_TO_STORE - 1) {
			filter.known_block(&block.hash());
			assert!(!filter.filter_block(&blocks[0].hash()));
		}

		filter.known_block(&blocks[MAX_LAST_BLOCKS_TO_STORE].hash());
		assert!(filter.filter_block(&blocks[0].hash()));
	}

	#[test]
	fn transaction_is_filtered_out_when_it_is_received_from_peer() {
		let transactions: Vec<Transaction> = (0..MAX_LAST_TRANSACTIONS_TO_STORE + 1)
			.map(|version| test_data::TransactionBuilder::with_version(version as i32).into())
			.collect();

		let mut filter = ConnectionFilter::default();
		assert!(filter.filter_transaction(&transactions[0].hash(), &transactions[0], None));

		filter.known_transaction(&transactions[0].hash());
		assert!(!filter.filter_transaction(&transactions[0].hash(), &transactions[0], None));

		for transaction in transactions.iter().skip(1).take(MAX_LAST_TRANSACTIONS_TO_STORE - 1) {
			filter.known_transaction(&transaction.hash());
			assert!(!filter.filter_transaction(&transactions[0].hash(), &transactions[0], None));
		}

		filter.known_transaction(&transactions[MAX_LAST_TRANSACTIONS_TO_STORE].hash());
		assert!(filter.filter_transaction(&transactions[0].hash(), &transactions[0], None));
	}
}
