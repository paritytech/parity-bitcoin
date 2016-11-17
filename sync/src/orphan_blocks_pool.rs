use std::collections::{HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use linked_hash_map::LinkedHashMap;
use time;
use chain::Block;
use primitives::hash::H256;

#[derive(Debug)]
/// Storage for blocks, for which we have no parent yet.
/// Blocks from this storage are either moved to verification queue, or removed at all.
pub struct OrphanBlocksPool {
	/// Blocks from requested_hashes, but received out-of-order.
	orphaned_blocks: HashMap<H256, HashMap<H256, Block>>,
	/// Blocks that we have received without requesting with receiving time.
	unknown_blocks: LinkedHashMap<H256, f64>,
}

impl OrphanBlocksPool {
	/// Create new pool
	pub fn new() -> Self {
		OrphanBlocksPool {
			orphaned_blocks: HashMap::new(),
			unknown_blocks: LinkedHashMap::new(),
		}
	}

	#[cfg(test)]
	/// Get total number of blocks in pool
	pub fn len(&self) -> usize {
		self.orphaned_blocks.len()
	}

	/// Check if block with given hash is stored as unknown in this pool
	pub fn contains_unknown_block(&self, hash: &H256) -> bool {
		self.unknown_blocks.contains_key(hash)
	}

	/// Get unknown blocks in the insertion order
	pub fn unknown_blocks(&self) -> &LinkedHashMap<H256, f64> {
		&self.unknown_blocks
	}

	/// Insert orphaned block, for which we have already requested its parent block
	pub fn insert_orphaned_block(&mut self, hash: H256, block: Block) {
		self.orphaned_blocks
			.entry(block.block_header.previous_header_hash.clone())
			.or_insert_with(HashMap::new)
			.insert(hash, block);
	}

	/// Insert unknown block, for which we know nothing about its parent block
	pub fn insert_unknown_block(&mut self, hash: H256, block: Block) {
		let previous_value = self.unknown_blocks.insert(hash.clone(), time::precise_time_s());
		assert_eq!(previous_value, None);

		self.insert_orphaned_block(hash, block);
	}

	/// Remove all blocks, which are not-unknown
	pub fn remove_known_blocks(&mut self) -> Vec<H256> {
		let orphans_to_remove: HashSet<_> = self.orphaned_blocks.values()
			.flat_map(|v| v.iter().map(|e| e.0.clone()))
			.filter(|h| !self.unknown_blocks.contains_key(h))
			.collect();
		self.remove_blocks(&orphans_to_remove);
		orphans_to_remove.into_iter().collect()
	}

	/// Remove all blocks, depending on this parent
	pub fn remove_blocks_for_parent(&mut self, hash: &H256) -> Vec<(H256, Block)> {
		let mut queue: VecDeque<H256> = VecDeque::new();
		queue.push_back(hash.clone());

		let mut removed: Vec<(H256, Block)> = Vec::new();
		while let Some(parent_hash) = queue.pop_front() {
			if let Entry::Occupied(entry) = self.orphaned_blocks.entry(parent_hash) {
				let (_, orphaned) = entry.remove_entry();
				for orphaned_hash in orphaned.keys() {
					self.unknown_blocks.remove(orphaned_hash);
				}
				queue.extend(orphaned.keys().cloned());
				removed.extend(orphaned.into_iter());
			}
		}
		removed
	}

	/// Remove blocks with given hashes + all dependent blocks
	pub fn remove_blocks(&mut self, hashes: &HashSet<H256>) -> Vec<(H256, Block)> {
		// TODO: excess clone
		let mut removed: Vec<(H256, Block)> = Vec::new();
		let parent_orphan_keys: Vec<_> = self.orphaned_blocks.keys().cloned().collect();
		for parent_orphan_key in parent_orphan_keys {
			if let Entry::Occupied(mut orphan_entry) = self.orphaned_blocks.entry(parent_orphan_key) {
				let remove_entry = {
					let mut orphans = orphan_entry.get_mut();
					let orphans_keys: HashSet<H256> = orphans.keys().cloned().collect();
					for orphan_to_remove in orphans_keys.intersection(hashes) {
						self.unknown_blocks.remove(orphan_to_remove);
						removed.push((orphan_to_remove.clone(),
							orphans.remove(orphan_to_remove).expect("iterating by intersection of orphans keys with hashes; removing from orphans; qed")
						));
					}
					orphans.is_empty()
				};
				
				if remove_entry {
					orphan_entry.remove_entry();
				}
			}
		}

		// also delete all children
		for hash in hashes.iter() {
			removed.extend(self.remove_blocks_for_parent(hash));
		}

		removed
	}
}

#[cfg(test)]
mod tests {
	use std::collections::HashSet;
	use test_data;
	use primitives::hash::H256;
	use chain::RepresentH256;
	use super::OrphanBlocksPool;

	#[test]
	fn orphan_block_pool_empty_on_start() {
		let pool = OrphanBlocksPool::new();
		assert_eq!(pool.len(), 0);
	}

	#[test]
	fn orphan_block_pool_insert_orphan_block() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();

		pool.insert_orphaned_block(b1_hash.clone(), b1);

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 0);
	}

	#[test]
	fn orphan_block_pool_insert_unknown_block() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();

		pool.insert_unknown_block(b1_hash.clone(), b1);

		assert_eq!(pool.len(), 1);
		assert!(pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_known_blocks() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h169();
		let b2_hash = b2.hash();

		pool.insert_orphaned_block(b1_hash.clone(), b1);
		pool.insert_unknown_block(b2_hash.clone(), b2);

		assert_eq!(pool.len(), 2);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);

		pool.remove_known_blocks();

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_blocks_for_parent() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h169();
		let b2_hash = b2.hash();
		let b3 = test_data::block_h2();
		let b3_hash = b3.hash();

		pool.insert_orphaned_block(b1_hash.clone(), b1);
		pool.insert_unknown_block(b2_hash.clone(), b2);
		pool.insert_orphaned_block(b3_hash.clone(), b3);

		let removed = pool.remove_blocks_for_parent(&test_data::genesis().hash());
		assert_eq!(removed.len(), 2);
		assert_eq!(removed[0].0, b1_hash);
		assert_eq!(removed[1].0, b3_hash);

		assert_eq!(pool.len(), 1);
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert!(pool.contains_unknown_block(&b2_hash));
		assert!(!pool.contains_unknown_block(&b1_hash));
		assert_eq!(pool.unknown_blocks().len(), 1);
	}

	#[test]
	fn orphan_block_pool_remove_blocks() {
		let mut pool = OrphanBlocksPool::new();
		let b1 = test_data::block_h1();
		let b1_hash = b1.hash();
		let b2 = test_data::block_h2();
		let b2_hash = b2.hash();
		let b3 = test_data::block_h169();
		let b3_hash = b3.hash();
		let b4 = test_data::block_h170();
		let b4_hash = b4.hash();
		let b5 = test_data::block_h181();
		let b5_hash = b5.hash();

		pool.insert_orphaned_block(b1_hash.clone(), b1);
		pool.insert_orphaned_block(b2_hash.clone(), b2);
		pool.insert_orphaned_block(b3_hash.clone(), b3);
		pool.insert_orphaned_block(b4_hash.clone(), b4);
		pool.insert_orphaned_block(b5_hash.clone(), b5);

		let mut blocks_to_remove: HashSet<H256> = HashSet::new();
		blocks_to_remove.insert(b1_hash.clone());
		blocks_to_remove.insert(b3_hash.clone());

		let removed = pool.remove_blocks(&blocks_to_remove);
		assert_eq!(removed.len(), 4);
		assert!(removed.iter().any(|&(ref h, _)| h == &b1_hash));
		assert!(removed.iter().any(|&(ref h, _)| h == &b2_hash));
		assert!(removed.iter().any(|&(ref h, _)| h == &b3_hash));
		assert!(removed.iter().any(|&(ref h, _)| h == &b4_hash));

		assert_eq!(pool.len(), 1);
	}
}
