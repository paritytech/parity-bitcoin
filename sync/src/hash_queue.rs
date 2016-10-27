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
	Inside,
}

#[derive(Clone)]
pub struct HashQueue {
	queue: VecDeque<H256>,
	set: HashSet<H256>
}

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

	pub fn len(&self) -> usize {
		self.queue.len()
	}

	pub fn is_empty(&self) -> bool {
		self.queue.is_empty()
	}

	pub fn back<'a>(&'a self) -> Option<&'a H256> {
		self.queue.back()
	}

	pub fn contains(&self, hash: &H256) -> bool {
		self.set.contains(hash)
	}

	pub fn pop_front(&mut self) -> Option<H256> {
		match self.queue.pop_front() {
			Some(hash) => {
				self.set.remove(&hash);
				Some(hash)
			},
			None => None,
		}
	}

	pub fn pop_front_n(&mut self, n: usize) -> Vec<H256> {
		let mut result: Vec<H256> = Vec::new();
		for _ in 0..n {
			match self.pop_front() {
				Some(hash) => result.push(hash),
				None => return result,
			}
		}
		result
	}

	pub fn push_back(&mut self, hash: H256) {
		if !self.set.insert(hash.clone()) {
			panic!("must be checked by caller");
		}
		self.queue.push_back(hash);
	}

	pub fn push_back_n(&mut self, hashes: Vec<H256>) {
		for hash in hashes {
			self.push_back(hash);
		}
	}

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
				return HashPosition::Inside;
			}
		}

		// unreachable because hash is not missing, not at the front and not inside
		unreachable!()
	}
}

impl Index<usize> for HashQueue {
	type Output = H256;

	fn index(&self, index: usize) -> &Self::Output {
		&self.queue[index]
	}
}

impl HashQueueChain {
	pub fn with_number_of_queues(number_of_queues: usize) -> Self {
		assert!(number_of_queues != 0);
		HashQueueChain {
			chain: repeat(HashQueue::new()).take(number_of_queues).collect(),
		}
	}

	pub fn len(&self) -> usize {
		self.chain.iter().fold(0, |total, chain| total + chain.len())
	}

	pub fn len_of(&self, chain_index: usize) -> usize {
		self.chain[chain_index].len()
	}

	pub fn is_empty_at(&self, chain_index: usize) -> bool {
		self.chain[chain_index].is_empty()
	}

	pub fn back(&self) -> Option<H256> {
		let mut queue_index = self.chain.len() - 1;
		loop {
			let ref queue = self.chain[queue_index];
			let queue_back = queue.back();
			if queue_back.is_some() {
				return queue_back.cloned();
			}

			queue_index = queue_index - 1;
			if queue_index == 0 {
				return None;
			}
		}
	}

	pub fn is_contained_in(&self, queue_index: usize, hash: &H256) -> bool {
		self.chain[queue_index].contains(hash)
	}

	pub fn contains_in(&self, hash: &H256) -> Option<usize> {
		for i in 0..self.chain.len() {
			if self.chain[i].contains(hash) {
				return Some(i);
			}
		}
		None
	}

	pub fn pop_front_n_at(&mut self, queue_index: usize, n: usize) -> Vec<H256> {
		self.chain[queue_index].pop_front_n(n)
	}

	pub fn push_back_n_at(&mut self, queue_index: usize, hashes: Vec<H256>) {
		self.chain[queue_index].push_back_n(hashes)
	}

	pub fn remove_at(&mut self, queue_index: usize, hash: &H256) -> HashPosition {
		self.chain[queue_index].remove(hash)
	}
}

impl Index<usize> for HashQueueChain {
	type Output = H256;

	fn index<'a>(&'a self, mut index: usize) -> &'a Self::Output {
		for queue in self.chain.iter() {
			let queue_len = queue.len();
			if index < queue_len {
				return &queue[index];
			}

			index -= queue_len;
		}

		panic!("invalid index");
	}
}
