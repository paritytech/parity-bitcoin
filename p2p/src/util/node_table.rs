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

#[derive(PartialEq, Eq, Clone)]
struct NodeByScore(Node);

impl From<Node> for NodeByScore {
	fn from(node: Node) -> Self {
		NodeByScore(node)
	}
}

impl PartialOrd for NodeByScore {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if self.0.failures == other.0.failures {
			other.0.time.partial_cmp(&self.0.time)
		} else {
			self.0.failures.partial_cmp(&other.0.failures)
		}
	}
}

impl Ord for NodeByScore {
	fn cmp(&self, other: &Self) -> Ordering {
		if self.0.failures == other.0.failures {
			other.0.time.cmp(&self.0.time)
		} else {
			self.0.failures.cmp(&other.0.failures)
		}
	}
}

#[derive(PartialEq, Eq, Clone)]
struct NodeByTime(Node);

impl From<Node> for NodeByTime {
	fn from(node: Node) -> Self {
		NodeByTime(node)
	}
}

impl PartialOrd for NodeByTime {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		other.0.time.partial_cmp(&self.0.time)
	}
}

impl Ord for NodeByTime {
	fn cmp(&self, other: &Self) -> Ordering {
		other.0.time.cmp(&self.0.time)
	}
}

#[derive(Default)]
pub struct NodeTable<T = RealTime> where T: Time {
	/// Time source.
	time: T,
	/// Nodes by socket address.
	by_addr: HashMap<SocketAddr, Node>,
	/// Nodes sorted by score.
	by_score: BTreeSet<NodeByScore>,
	/// Nodes sorted by time.
	by_time: BTreeSet<NodeByTime>,
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
		self.by_score.insert(node.clone().into());
		self.by_time.insert(node.into());
	}

	/// Returnes most reliable nodes with desired services.
	pub fn nodes_with_services(&self, services: &Services, limit: usize) -> Vec<Node> {
		self.by_score.iter()
			.filter(|node| node.0.services.includes(services))
			.map(|node| node.0.clone())
			.take(limit)
			.collect()
	}

	/// Returns nodes active in last 3 hours (no more than 1000).
	pub fn recently_active_nodes(&self) -> Vec<Node> {
		let now = self.time.get().sec;
		let duration = 60 * 60 * 3;
		self.by_time.iter()
			.take_while(|node| node.0.time + duration > now)
			.map(|node| node.0.clone())
			.take(1000)
			.collect()
	}

	/// Marks address as recently used.
	pub fn note_used(&mut self, addr: &SocketAddr) {
		if let Some(ref mut node) = self.by_addr.get_mut(addr) {
			assert!(self.by_score.remove(&node.clone().into()));
			assert!(self.by_time.remove(&node.clone().into()));
			node.time = self.time.get().sec;
			self.by_score.insert(node.clone().into());
			self.by_time.insert(node.clone().into());
		}
	}

	/// Notes failure.
	pub fn note_failure(&mut self, addr: &SocketAddr) {
		if let Some(ref mut node) = self.by_addr.get_mut(addr) {
			assert!(self.by_score.remove(&node.clone().into()));
			assert!(self.by_time.remove(&node.clone().into()));
			node.failures += 1;
			self.by_score.insert(node.clone().into());
			self.by_time.insert(node.clone().into());
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

		let nodes = table.recently_active_nodes();
		assert_eq!(nodes.len(), 5);

		assert_eq!(nodes[0].addr, s1);
		assert_eq!(nodes[0].time, 7);
		assert_eq!(nodes[0].failures, 0);

		assert_eq!(nodes[1].addr, s4);
		assert_eq!(nodes[1].time, 6);
		assert_eq!(nodes[1].failures, 0);

		assert_eq!(nodes[2].addr, s2);
		assert_eq!(nodes[2].time, 5);
		assert_eq!(nodes[2].failures, 1);

		assert_eq!(nodes[3].addr, s3);
		assert_eq!(nodes[3].time, 3);
		assert_eq!(nodes[3].failures, 1);

		assert_eq!(nodes[4].addr, s0);
		assert_eq!(nodes[4].time, 0);
		assert_eq!(nodes[4].failures, 0);
	}

}
