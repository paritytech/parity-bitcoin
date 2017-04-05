use std::cmp::min;
use bit_vec::BitVec;
use chain::merkle_node_hash;
use primitives::hash::H256;

/// Partial merkle tree
pub struct PartialMerkleTree {
	/// Total number of transactions
	pub tx_count: usize,
	/// Nodes hashes
	pub hashes: Vec<H256>,
	/// Match flags
	pub flags: BitVec,
}

/// Partial merkle tree parse result
#[cfg(test)]
pub struct ParsedPartialMerkleTree {
	/// Merkle root
	pub root: H256,
	/// Matched hashes
	pub hashes: Vec<H256>,
	/// Match flags
	pub flags: BitVec,
}

/// Build partial merkle tree
pub fn build_partial_merkle_tree(tx_hashes: Vec<H256>, tx_matches: BitVec) -> PartialMerkleTree {
	PartialMerkleTreeBuilder::build(tx_hashes, tx_matches)
}

/// Parse partial merkle tree
#[cfg(test)]
pub fn parse_partial_merkle_tree(tree: PartialMerkleTree) -> Result<ParsedPartialMerkleTree, String> {
	PartialMerkleTreeBuilder::parse(tree)
}

/// Service structure to construct `merkleblock` message.
struct PartialMerkleTreeBuilder {
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

impl PartialMerkleTree {
	/// Create new merkle tree with given data
	pub fn new(tx_count:usize, hashes: Vec<H256>, flags: BitVec) -> Self {
		PartialMerkleTree {
			tx_count: tx_count,
			hashes: hashes,
			flags: flags,
		}
	}
}

#[cfg(test)]
impl ParsedPartialMerkleTree {
	pub fn new(root: H256, hashes: Vec<H256>, flags: BitVec) -> Self {
		ParsedPartialMerkleTree {
			root: root,
			hashes: hashes,
			flags: flags,
		}
	}
}

impl PartialMerkleTreeBuilder {
	/// Build partial merkle tree as described here:
	/// https://bitcoin.org/en/developer-reference#creating-a-merkleblock-message
	pub fn build(all_hashes: Vec<H256>, all_matches: BitVec) -> PartialMerkleTree {
		let mut partial_merkle_tree = PartialMerkleTreeBuilder {
			all_len: all_hashes.len(),
			all_hashes: all_hashes,
			all_matches: all_matches,
			hashes: Vec::new(),
			matches: BitVec::new(),
		};
		partial_merkle_tree.build_tree();
		PartialMerkleTree::new(partial_merkle_tree.all_len, partial_merkle_tree.hashes, partial_merkle_tree.matches)
	}

	#[cfg(test)]
	/// Parse partial merkle tree as described here:
	/// https://bitcoin.org/en/developer-reference#parsing-a-merkleblock-message
	pub fn parse(tree: PartialMerkleTree) -> Result<ParsedPartialMerkleTree, String> {
		let mut partial_merkle_tree = PartialMerkleTreeBuilder {
			all_len: tree.tx_count,
			all_hashes: Vec::new(),
			all_matches: BitVec::from_elem(tree.tx_count, false),
			hashes: tree.hashes,
			matches: tree.flags,
		};

		let merkle_root = try!(partial_merkle_tree.parse_tree());
		Ok(ParsedPartialMerkleTree::new(merkle_root, partial_merkle_tree.all_hashes, partial_merkle_tree.all_matches))
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
mod tests {
	extern crate test_data;

	use chain::{Transaction, merkle_root};
	use primitives::hash::H256;
	use super::{build_partial_merkle_tree, parse_partial_merkle_tree};

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
				let partial_tree = build_partial_merkle_tree(hashes.clone(), matches.clone());
				// parse tree back
				let parsed_tree = parse_partial_merkle_tree(partial_tree).expect("no error");

				assert_eq!(matched_hashes, parsed_tree.hashes);
				assert_eq!(matches, parsed_tree.flags);
				assert_eq!(merkle_root, parsed_tree.root);
			}
		}
	}
}
