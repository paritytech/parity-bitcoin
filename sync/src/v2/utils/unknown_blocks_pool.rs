use std::collections::{HashMap, HashSet, VecDeque};
use std::collections::hash_map::Entry;
use linked_hash_map::LinkedHashMap;
use time;
use chain::IndexedBlock;
use orphan_blocks_pool::OrphanBlocksPool;
use primitives::hash::H256;

/// Block, for which parent block is unknown.
#[derive(Debug)]
pub struct UnknownBlock {
	/// Time when this block was inserted to the pool
	pub insertion_time: f64,
	/// Block itself
	pub block: IndexedBlock,
}

/// Storage for blocks, for which parent block is unknown.
#[derive(Debug)]
pub struct UnknownBlocksPool {
	/// { Parent block hash: { Block hash : block } }.
	by_parent_hash: HashMap<H256, HashMap<H256, UnknownBlock>>,
	/// { Block hash: parent block hash } ordered by insertion time.
	by_insertion_time: LinkedHashMap<H256, H256>,
}

impl UnknownBlocksPool {
	/// Create new pool
	pub fn new() -> Self {
		UnknownBlocksPool {
			by_parent_hash: HashMap::new(),
			by_insertion_time: LinkedHashMap::new(),
		}
	}

	/// Get total number of blocks in pool
	pub fn len(&self) -> usize {
		self.by_parent_hash.len()
	}

	/// Check if pool already contains this block
	pub fn contains_block(&self, block: &IndexedBlock) -> bool {
		self.by_insertion_time.contains_key(&block.header.hash)
	}

	/// Insert unknown block
	pub fn insert_block(&mut self, block: IndexedBlock) {
		self.by_parent_hash
			.entry(block.header.raw.previous_header_hash.clone())
			.or_insert_with(HashMap::new)
			.insert(block.header.hash.clone(), block.into());
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
				removed.extend(orphaned.drain().map(|(_, v)| v.block));
			}
		}
		removed
	}
}

impl From<IndexedBlock> for UnknownBlock {
	fn from(block: IndexedBlock) -> Self {
		UnknownBlock {
			insertion_time: time::precise_time_s(),
			block: block,
		}
	}
}

#[cfg(test)]
mod tests {
}
