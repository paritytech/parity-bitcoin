use std::collections::{HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use linked_hash_map::LinkedHashMap;
use time;
use chain::IndexedBlock;
use primitives::hash::H256;

/// Storage for blocks, for which parent block has been requested, but not received yet.
#[derive(Debug)]
pub struct OrphanBlocksPool {
	/// { Parent block hash: { Block hash : block } }.
	by_parent_hash: HashMap<H256, HashMap<H256, IndexedBlock>>,
}

impl OrphanBlocksPool {
	/// Create new pool
	pub fn new() -> Self {
		OrphanBlocksPool {
			by_parent_hash: HashMap::new(),
		}
	}

	/// Get total number of blocks in pool
	pub fn len(&self) -> usize {
		self.by_parent_hash.len()
	}

	/// Insert orphaned block, for which we have already requested its parent block
	pub fn insert_orphaned_block(&mut self, block: IndexedBlock) {
		self.by_parent_hash
			.entry(block.header.raw.previous_header_hash.clone())
			.or_insert_with(HashMap::new)
			.insert(block.header.hash.clone(), block);
	}

	/// Remove all blocks
	pub fn clear(&mut self) -> Vec<H256> {
		self.by_parent_hash.drain()
			.flat_map(|(_, mut v)| v.drain().map(|(k, _)| k).collect::<Vec<_>>())
			.collect()
	}

	/// Remove all blocks, depending on this parent
	pub fn remove_blocks_for_parent(&mut self, hash: &H256) -> Vec<IndexedBlock> {
		let mut queue: VecDeque<H256> = VecDeque::new();
		queue.push_back(hash.clone());

		let mut removed: Vec<IndexedBlock> = Vec::new();
		while let Some(parent_hash) = queue.pop_front() {
			if let Entry::Occupied(entry) = self.by_parent_hash.entry(parent_hash) {
				let (_, mut orphaned) = entry.remove_entry();
				queue.extend(orphaned.keys().cloned());
				removed.extend(orphaned.drain().map(|(_, v)| v));
			}
		}
		removed
	}

	/// Remove blocks with given hashes + all dependent blocks
	pub fn remove_blocks(&mut self, hashes: &HashSet<H256>) -> Vec<IndexedBlock> {
		// TODO: excess clone
		let mut removed: Vec<IndexedBlock> = Vec::new();
		let parent_orphan_keys: Vec<_> = self.by_parent_hash.keys().cloned().collect();
		for parent_orphan_key in parent_orphan_keys {
			if let Entry::Occupied(mut orphan_entry) = self.by_parent_hash.entry(parent_orphan_key) {
				let remove_entry = {
					let mut orphans = orphan_entry.get_mut();
					let orphans_keys: HashSet<H256> = orphans.keys().cloned().collect();
					for orphan_to_remove in orphans_keys.intersection(hashes) {
						removed.push(
							orphans.remove(orphan_to_remove)
								.expect("iterating by intersection of orphans keys with hashes; removing from orphans; qed")
						);
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

		assert_eq!(pool.len(), 0);
		pool.insert_orphaned_block(b1.into());
		assert_eq!(pool.len(), 1);
		pool.clear();
		assert_eq!(pool.len(), 0);
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

		pool.insert_orphaned_block(b1.into());
		pool.insert_orphaned_block(b2.into());
		pool.insert_orphaned_block(b3.into());

		let removed = pool.remove_blocks_for_parent(&test_data::genesis().hash());
		assert_eq!(removed.len(), 2);
		assert_eq!(removed[0].header.hash, b1_hash);
		assert_eq!(removed[1].header.hash, b3_hash);

		assert_eq!(pool.len(), 1);
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

		pool.insert_orphaned_block(b1.into());
		pool.insert_orphaned_block(b2.into());
		pool.insert_orphaned_block(b3.into());
		pool.insert_orphaned_block(b4.into());
		pool.insert_orphaned_block(b5.into());

		let mut blocks_to_remove: HashSet<H256> = HashSet::new();
		blocks_to_remove.insert(b1_hash.clone());
		blocks_to_remove.insert(b3_hash.clone());

		let removed = pool.remove_blocks(&blocks_to_remove);
		assert_eq!(removed.len(), 4);
		assert!(removed.iter().any(|ref b| &b.header.hash == &b1_hash));
		assert!(removed.iter().any(|ref b| &b.header.hash == &b2_hash));
		assert!(removed.iter().any(|ref b| &b.header.hash == &b3_hash));
		assert!(removed.iter().any(|ref b| &b.header.hash == &b4_hash));

		assert_eq!(pool.len(), 1);
	}
}
