use std::collections::{HashMap, BTreeSet};
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::cmp::{PartialOrd, Ord, Ordering};
use message::common::{Services, NetAddress};
use message::types::addr::AddressEntry;
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

impl Node {
	pub fn address(&self) -> SocketAddr {
		self.addr
	}
}

impl From<AddressEntry> for Node {
	fn from(entry: AddressEntry) -> Self {
		Node {
			addr: SocketAddr::new(entry.address.address.into(), entry.address.port.into()),
			time: entry.timestamp as i64,
			services: entry.address.services,
			failures: 0,
		}
	}
}

impl From<Node> for AddressEntry {
	fn from(node: Node) -> Self {
		AddressEntry {
			timestamp: node.time as u32,
			address: NetAddress {
				services: node.services,
				address: node.addr.ip().into(),
				port: node.addr.port().into(),
			}
		}
	}
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
		let now = self.time.get().sec;
		match self.by_addr.entry(addr) {
			Entry::Occupied(mut entry) => {
				let old = entry.get_mut();
				assert!(self.by_score.remove(&old.clone().into()));
				assert!(self.by_time.remove(&old.clone().into()));
				old.time = now;
				old.services = services;
				self.by_score.insert(old.clone().into());
				self.by_time.insert(old.clone().into());
			},
			Entry::Vacant(entry) => {
				let node = Node {
					addr: addr,
					time: now,
					services: services,
					failures: 0,
				};
				self.by_score.insert(node.clone().into());
				self.by_time.insert(node.clone().into());
				entry.insert(node);
			}
		}
	}

	/// Inserts many new addresses into node table.
	/// Used in `addr` request handler.
	/// Discards all nodes with timestamp newer than current time.
	pub fn insert_many(&mut self, nodes: Vec<Node>) {
		// discard all nodes with timestamp newer than current time.
		let now = self.time.get().sec;
		let iter = nodes.into_iter()
			.filter(|node| node.time <= now);

		// iterate over the rest
		for node in iter {
			match self.by_addr.entry(node.addr) {
				Entry::Occupied(mut entry) => {
					let old = entry.get_mut();
					// we've already seen this node
					if old.time < node.time {
						assert!(self.by_score.remove(&old.clone().into()));
						assert!(self.by_time.remove(&old.clone().into()));
						// update node info
						old.time = node.time;
						old.services = node.services;
						self.by_score.insert(old.clone().into());
						self.by_time.insert(old.clone().into());
					}
				},
				Entry::Vacant(entry)=> {
					// it's first time we see this node
					self.by_score.insert(node.clone().into());
					self.by_time.insert(node.clone().into());
					entry.insert(node);
				}
			}
		}
	}

	/// Returnes most reliable nodes with desired services.
	pub fn nodes_with_services(&self, services: &Services, limit: usize) -> Vec<Node> {
		self.by_score.iter()
			.filter(|node| node.0.services.includes(services))
			.map(|node| node.0.clone())
			.take(limit)
			.collect()
	}

	/// Returns most recently active nodes.
	///
	/// The documenation says:
	/// "Non-advertised nodes should be forgotten after typically 3 hours"
	/// but bitcoin client still advertises them even after a month.
	/// Let's do the same.
	///
	/// https://en.bitcoin.it/wiki/Protocol_documentation#addr
	pub fn recently_active_nodes(&self) -> Vec<Node> {
		self.by_time.iter()
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
