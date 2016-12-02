use std::ops::Index;
use std::collections::{VecDeque, HashSet};
use std::iter::repeat;
use primitives::hash::H256;

/// Block position
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum HashPosition {
	/// Block is not in the queue
	Missing,
	/// Block is at the front of the queue
	Front,
	/// Block is somewhere inside in the queue
	Inside(u32),
}

/// Ordered queue with O(1) contains() && random access operations cost.
#[derive(Debug, Clone)]
pub struct HashQueue {
	queue: VecDeque<H256>,
	set: HashSet<H256>,
}

/// Chain of linked queues. First queue has index zero.
#[derive(Debug)]
pub struct HashQueueChain {
	chain: Vec<HashQueue>,
}

impl HashQueue {
	pub fn new() -> Self {
		HashQueue {
			queue: VecDeque::new(),
			set: HashSet::new(),
		}
	}

	/// Clears the queue
	pub fn clear(&mut self) {
		self.set.clear();
		self.queue.clear();
	}

	/// Returns len of the given queue.
	pub fn len(&self) -> u32 {
		self.queue.len() as u32
	}

	/// Returns front element from the given queue.
	pub fn front(&self) -> Option<H256> {
		self.queue.front().cloned()
	}

	/// Returns back element from the given queue.
	pub fn back(&self) -> Option<H256> {
		self.queue.back().cloned()
	}

	/// Returns position of the element in the queue
	pub fn position(&self, hash: &H256) -> Option<u32> {
		self.queue.iter().enumerate()
			.filter_map(|(pos, h)| if hash == h { Some(pos as u32) } else { None })
			.nth(0)
	}

	/// Returns element at position
	pub fn at(&self, position: u32) -> Option<H256> {
		self.queue.get(position as usize).cloned()
	}

	/// Returns previous-to back element from the given queue.
	pub fn pre_back(&self) -> Option<H256> {
		let queue_len = self.queue.len();
		if queue_len <= 1 {
			return None;
		}
		Some(self.queue[queue_len - 2].clone())
	}

	/// Returns true if queue contains element.
	pub fn contains(&self, hash: &H256) -> bool {
		self.set.contains(hash)
	}

	/// Returns n elements from the front of the queue
	pub fn front_n(&self, n: u32) -> Vec<H256> {
		self.queue.iter().cloned().take(n as usize).collect()
	}

	/// Removes element from the front of the queue.
	pub fn pop_front(&mut self) -> Option<H256> {
		match self.queue.pop_front() {
			Some(hash) => {
				self.set.remove(&hash);
				Some(hash)
			},
			None => None,
		}
	}

	/// Removes n elements from the front of the queue.
	pub fn pop_front_n(&mut self, n: u32) -> Vec<H256> {
		let mut result: Vec<H256> = Vec::new();
		for _ in 0..n {
			match self.pop_front() {
				Some(hash) => result.push(hash),
				None => return result,
			}
		}
		result
	}

	/// Removes element from the back of the queue.
	pub fn pop_back(&mut self) -> Option<H256> {
		match self.queue.pop_back() {
			Some(hash) => {
				self.set.remove(&hash);
				Some(hash)
			},
			None => None,
		}
	}


	/// Adds element to the back of the queue.
	pub fn push_back(&mut self, hash: H256) {
		if !self.set.insert(hash.clone()) {
			panic!("must be checked by caller");
		}
		self.queue.push_back(hash);
	}

	/// Adds elements to the back of the queue.
	pub fn push_back_n(&mut self, hashes: Vec<H256>) {
		for hash in hashes {
			self.push_back(hash);
		}
	}

	/// Removes element from the queue, returning its position.
	pub fn remove(&mut self, hash: &H256) -> HashPosition {
		if !self.set.remove(hash) {
			return HashPosition::Missing;
		}

		if self.queue.front().expect("checked one line above") == hash {
			self.queue.pop_front();
			return HashPosition::Front;
		}

		for i in 0..self.queue.len() {
			if self.queue[i] == *hash {
				self.queue.remove(i);
				return HashPosition::Inside(i as u32);
			}
		}

		// unreachable because hash is not missing, not at the front and not inside
		unreachable!()
	}

	/// Removes all elements from the queue.
	pub fn remove_all(&mut self) -> VecDeque<H256> {
		use std::mem::replace;

		self.set.clear();
		replace(&mut self.queue, VecDeque::new())
	}
}

impl Index<u32> for HashQueue {
	type Output = H256;

	fn index(&self, index: u32) -> &Self::Output {
		&self.queue[index as usize]
	}
}

impl HashQueueChain {
	/// Creates chain with given number of queues.
	pub fn with_number_of_queues(number_of_queues: usize) -> Self {
		assert!(number_of_queues != 0);
		HashQueueChain {
			chain: repeat(HashQueue::new()).take(number_of_queues).collect(),
		}
	}

	/// Returns length of the whole chain.
	pub fn len(&self) -> u32 {
		self.chain.iter().fold(0, |total, chain| total + chain.len())
	}

	/// Returns length of the given queue.
	pub fn len_of(&self, queue_index: usize) -> u32 {
		self.chain[queue_index].len()
	}

	/// Returns element at the given position
	pub fn at(&self, mut index: u32) -> Option<H256> {
		for queue in &self.chain {
			let queue_len = queue.len();
			if index < queue_len {
				return queue.at(index);
			}

			index -= queue_len;
		}

		None
	}

	/// Returns element at the front of the given queue.
	pub fn front_at(&self, queue_index: usize) -> Option<H256> {
		let queue = &self.chain[queue_index];
		queue.front()
	}

	/// Returns element at the front of the given queue.
	pub fn back_at(&self, queue_index: usize) -> Option<H256> {
		let queue = &self.chain[queue_index];
		queue.back()
	}

	/// Returns previous-to back element from the given queue.
	pub fn pre_back_at(&self, chain_index: usize) -> Option<H256> {
		let queue = &self.chain[chain_index];
		queue.pre_back()
	}

	/// Returns the back of the whole chain.
	pub fn back(&self) -> Option<H256> {
		let mut queue_index = self.chain.len() - 1;
		loop {
			let queue = &self.chain[queue_index];
			let queue_back = queue.back();
			if queue_back.is_some() {
				return queue_back;
			}

			queue_index -= 1;
			if queue_index == 0 {
				return None;
			}
		}
	}

	/// Checks if hash is contained in given queue.
	#[cfg(test)]
	pub fn is_contained_in(&self, queue_index: usize, hash: &H256) -> bool {
		self.chain[queue_index].contains(hash)
	}

	/// Returns the index of queue, hash is contained in.
	pub fn contains_in(&self, hash: &H256) -> Option<usize> {
		for i in 0..self.chain.len() {
			if self.chain[i].contains(hash) {
				return Some(i);
			}
		}
		None
	}

	/// Returns n elements from the front of the given queue
	pub fn front_n_at(&self, queue_index: usize, n: u32) -> Vec<H256> {
		self.chain[queue_index].front_n(n)
	}

	/// Remove a number of hashes from the front of the given queue.
	pub fn pop_front_n_at(&mut self, queue_index: usize, n: u32) -> Vec<H256> {
		self.chain[queue_index].pop_front_n(n)
	}

	/// Push hash onto the back of the given queue.
	pub fn push_back_at(&mut self, queue_index: usize, hash: H256) {
		self.chain[queue_index].push_back(hash)
	}

	/// Push a number of hashes onto the back of the given queue.
	pub fn push_back_n_at(&mut self, queue_index: usize, hashes: Vec<H256>) {
		self.chain[queue_index].push_back_n(hashes)
	}

	/// Remove hash from given queue.
	pub fn remove_at(&mut self, queue_index: usize, hash: &H256) -> HashPosition {
		self.chain[queue_index].remove(hash)
	}

	/// Remove all items from given queue.
	pub fn remove_all_at(&mut self, queue_index: usize) -> VecDeque<H256> {
		self.chain[queue_index].remove_all()
	}
}

impl Index<u32> for HashQueueChain {
	type Output = H256;

	fn index(&self, mut index: u32) -> &Self::Output {
		for queue in &self.chain {
			let queue_len = queue.len();
			if index < queue_len {
				return &queue[index];
			}

			index -= queue_len;
		}

		panic!("invalid index");
	}
}

#[cfg(test)]
mod tests {
	use super::{HashQueue, HashQueueChain, HashPosition};
	use primitives::hash::H256;

	#[test]
	fn hash_queue_empty() {
		let mut queue = HashQueue::new();
		assert_eq!(queue.len(), 0);
		assert_eq!(queue.front(), None);
		assert_eq!(queue.back(), None);
		assert_eq!(queue.pre_back(), None);
		assert_eq!(queue.contains(&"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into()), false);
		assert_eq!(queue.pop_front(), None);
		assert_eq!(queue.pop_front_n(100), vec![]);
		assert_eq!(queue.remove(&"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into()), HashPosition::Missing);
	}

	#[test]
	fn hash_queue_chain_empty() {
		let mut chain = HashQueueChain::with_number_of_queues(3);
		assert_eq!(chain.len(), 0);
		assert_eq!(chain.len_of(0), 0);
		assert_eq!(chain.front_at(0), None);
		assert_eq!(chain.back_at(0), None);
		assert_eq!(chain.pre_back_at(0), None);
		assert_eq!(chain.back(), None);
		assert_eq!(chain.is_contained_in(0, &"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into()), false);
		assert_eq!(chain.contains_in(&"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into()), None);
		assert_eq!(chain.pop_front_n_at(0, 100), vec![]);
		assert_eq!(chain.remove_at(0, &"000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f".into()), HashPosition::Missing);
	}

	#[test]
	fn hash_queue_chain_not_empty() {
		let mut chain = HashQueueChain::with_number_of_queues(4);
		chain.push_back_n_at(0, vec![
			H256::from(0),
			H256::from(1),
			H256::from(2),
		]);
		chain.push_back_n_at(1, vec![
			H256::from(3),
			H256::from(4),
		]);
		chain.push_back_n_at(2, vec![
			H256::from(5),
		]);

		assert_eq!(chain.len(), 6);
		assert_eq!(chain.len_of(0), 3);
		assert_eq!(chain.len_of(1), 2);
		assert_eq!(chain.len_of(2), 1);
		assert_eq!(chain.len_of(3), 0);
		assert_eq!(chain.front_at(0), Some(H256::from(0)));
		assert_eq!(chain.front_at(1), Some(H256::from(3)));
		assert_eq!(chain.front_at(2), Some(H256::from(5)));
		assert_eq!(chain.front_at(3), None);
		assert_eq!(chain.back_at(0), Some(H256::from(2)));
		assert_eq!(chain.back_at(1), Some(H256::from(4)));
		assert_eq!(chain.back_at(2), Some(H256::from(5)));
		assert_eq!(chain.back_at(3), None);
		assert_eq!(chain.pre_back_at(0), Some(H256::from(1)));
		assert_eq!(chain.pre_back_at(1), Some(H256::from(3)));
		assert_eq!(chain.pre_back_at(2), None);
		assert_eq!(chain.pre_back_at(3), None);
		assert_eq!(chain.back(), Some(H256::from(5)));
		assert_eq!(chain.is_contained_in(0, &H256::from(2)), true);
		assert_eq!(chain.is_contained_in(1, &H256::from(2)), false);
		assert_eq!(chain.is_contained_in(2, &H256::from(2)), false);
		assert_eq!(chain.is_contained_in(3, &H256::from(2)), false);
		assert_eq!(chain.contains_in(&H256::from(2)), Some(0));
		assert_eq!(chain.contains_in(&H256::from(5)), Some(2));
		assert_eq!(chain.contains_in(&H256::from(9)), None);
	}

	#[test]
	fn hash_queue_front_n() {
		let mut queue = HashQueue::new();
		queue.push_back_n(vec![H256::from(0), H256::from(1)]);
		assert_eq!(queue.front_n(3), vec![H256::from(0), H256::from(1)]);
		assert_eq!(queue.front_n(3), vec![H256::from(0), H256::from(1)]);
		assert_eq!(queue.pop_front_n(3), vec![H256::from(0), H256::from(1)]);
		assert_eq!(queue.pop_front_n(3), vec![]);
	}
}
