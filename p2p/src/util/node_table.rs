use std::collections::{HashMap, BTreeSet};
use std::net::SocketAddr;
use std::cmp::{PartialOrd, Ord, Ordering};
use message::common::Services;
use util::time::{Time, RealTime};

#[derive(PartialEq, Eq, Clone)]
pub struct Node {
	/// Node address.
	addr: SocketAddr,
	/// Timestamp of last interaction with a node.
	time: i64,
	/// Services supported by the node.
	services: Services,
	/// Node failures counter.
	failures: u32,
}

impl PartialOrd for Node {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if self.failures == other.failures {
			self.time.partial_cmp(&other.time)
		} else {
			other.failures.partial_cmp(&self.failures)
		}
	}
}

impl Ord for Node {
	fn cmp(&self, other: &Self) -> Ordering {
		if self.failures == other.failures {
			self.time.cmp(&other.time)
		} else {
			other.failures.cmp(&self.failures)
		}
	}
}

#[derive(Default)]
pub struct NodeTable<T = RealTime> where T: Time {
	/// Time source.
	time: T,
	/// Nodes by socket address.
	by_addr: HashMap<SocketAddr, Node>,
	/// Nodes sorted by score.
	by_score: BTreeSet<Node>,
}

impl<T> NodeTable<T> where T: Time {
	/// Inserts new address and services pair into NodeTable.
	pub fn insert(&mut self, addr: SocketAddr, services: Services) {
		let failures = self.by_addr.get(&addr).map_or(0, |ref node| node.failures);

		let node = Node {
			addr: addr,
			time: self.time.get().sec,
			services: services,
			failures: failures,
		};

		self.by_addr.insert(addr, node.clone());
		self.by_score.insert(node);
	}

	/// Returnes most reliable nodes with desired services.
	pub fn nodes_with_services(&self, services: &Services, limit: usize) -> Vec<Node> {
		self.by_score.iter()
			.rev()
			.filter(|s| s.services.includes(services))
			.map(Clone::clone)
			.take(limit)
			.collect()
	}

	/// Marks address as recently used.
	pub fn note_used(&mut self, addr: &SocketAddr) {
		if let Some(ref mut node) = self.by_addr.get_mut(addr) {
			assert!(self.by_score.remove(node));
			node.time = self.time.get().sec;
			self.by_score.insert(node.clone());
		}
	}

	/// Notes failure.
	pub fn note_failure(&mut self, addr: &SocketAddr) {
		if let Some(ref mut node) = self.by_addr.get_mut(addr) {
			assert!(self.by_score.remove(node));
			node.failures += 1;
			self.by_score.insert(node.clone());
		}
	}
}

#[cfg(test)]
mod tests {
	use std::net::SocketAddr;
	use message::common::Services;
	use util::time::IncrementalTime;
	use super::NodeTable;

	#[test]
	fn test_node_table_insert() {
		let s0: SocketAddr = "127.0.0.1:8000".parse().unwrap();
		let s1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
		let s2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
		let mut table = NodeTable::<IncrementalTime>::default();
		table.insert(s0, Services::default());
		table.insert(s1, Services::default());
		table.insert(s2, Services::default());
		let nodes = table.nodes_with_services(&Services::default(), 2);
		assert_eq!(nodes.len(), 2);
		assert_eq!(nodes[0].addr, s2);
		assert_eq!(nodes[0].time, 2);
		assert_eq!(nodes[0].failures, 0);
		assert_eq!(nodes[1].addr, s1);
		assert_eq!(nodes[1].time, 1);
		assert_eq!(nodes[1].failures, 0);
	}

	#[test]
	fn test_node_table_note() {
		let s0: SocketAddr = "127.0.0.1:8000".parse().unwrap();
		let s1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
		let s2: SocketAddr = "127.0.0.1:8002".parse().unwrap();
		let s3: SocketAddr = "127.0.0.1:8003".parse().unwrap();
		let s4: SocketAddr = "127.0.0.1:8004".parse().unwrap();
		let mut table = NodeTable::<IncrementalTime>::default();
		table.insert(s0, Services::default());
		table.insert(s1, Services::default());
		table.insert(s2, Services::default());
		table.insert(s3, Services::default());
		table.insert(s4, Services::default());
		table.note_used(&s2);
		table.note_used(&s4);
		table.note_used(&s1);
		table.note_failure(&s2);
		table.note_failure(&s3);
		let nodes = table.nodes_with_services(&Services::default(), 10);
		assert_eq!(nodes.len(), 5);

		assert_eq!(nodes[0].addr, s1);
		assert_eq!(nodes[0].time, 7);
		assert_eq!(nodes[0].failures, 0);

		assert_eq!(nodes[1].addr, s4);
		assert_eq!(nodes[1].time, 6);
		assert_eq!(nodes[1].failures, 0);

		assert_eq!(nodes[2].addr, s0);
		assert_eq!(nodes[2].time, 0);
		assert_eq!(nodes[2].failures, 0);

		assert_eq!(nodes[3].addr, s2);
		assert_eq!(nodes[3].time, 5);
		assert_eq!(nodes[3].failures, 1);

		assert_eq!(nodes[4].addr, s3);
		assert_eq!(nodes[4].time, 3);
		assert_eq!(nodes[4].failures, 1);
	}
}
