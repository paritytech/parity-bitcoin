use std::{io, path, fs, net};
use std::collections::{HashSet, HashMap, BTreeSet};
use std::collections::hash_map::Entry;
use std::net::SocketAddr;
use std::cmp::{PartialOrd, Ord, Ordering};
use csv;
use message::common::{Services, NetAddress};
use message::types::addr::AddressEntry;
use util::time::{Time, RealTime};
use util::InternetProtocol;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Node {
	/// Node address.
	addr: SocketAddr,
	/// Timestamp of last interaction with a node.
	time: i64,
	/// Services supported by the node.
	services: Services,
	/// Is preferable node?
	is_preferable: bool,
	/// Node failures counter.
	failures: u32,
}

impl Node {
	pub fn address(&self) -> SocketAddr {
		self.addr
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

#[derive(Debug, PartialEq, Eq, Clone)]
struct NodeByScore(Node);

impl From<Node> for NodeByScore {
	fn from(node: Node) -> Self {
		NodeByScore(node)
	}
}

impl PartialOrd for NodeByScore {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if self.0.failures == other.0.failures {
			if self.0.is_preferable == other.0.is_preferable {
				if other.0.time == self.0.time {
					other.0.partial_cmp(&self.0)
				}
				else {
					other.0.time.partial_cmp(&self.0.time)
				}
			} else if self.0.is_preferable {
				return Some(Ordering::Less)
			} else {
				Some(Ordering::Greater)
			}
		} else {
			self.0.failures.partial_cmp(&other.0.failures)
		}
	}
}

impl Ord for NodeByScore {
	fn cmp(&self, other: &Self) -> Ordering {
		if self.0.failures == other.0.failures {
			if self.0.is_preferable == other.0.is_preferable {
				if other.0.time == self.0.time {
					other.0.cmp(&self.0)
				}
				else {
					other.0.time.cmp(&self.0.time)
				}
			} else if self.0.is_preferable {
				return Ordering::Less
			} else {
				Ordering::Greater
			}
		} else {
			self.0.failures.cmp(&other.0.failures)
		}
	}
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct NodeByTime(Node);

impl From<Node> for NodeByTime {
	fn from(node: Node) -> Self {
		NodeByTime(node)
	}
}

impl PartialOrd for NodeByTime {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		if other.0.time == self.0.time {
			other.0.partial_cmp(&self.0)
		}
		else {
			other.0.time.partial_cmp(&self.0.time)
		}
	}
}

impl Ord for NodeByTime {
	fn cmp(&self, other: &Self) -> Ordering {
		if other.0.time == self.0.time {
			other.0.cmp(&self.0)
		}
		else {
			other.0.time.cmp(&self.0.time)
		}
	}
}

impl Ord for Node {
	fn cmp(&self, other: &Self) -> Ordering {
		// some ordering using address as unique key
		match self.addr {
			SocketAddr::V4(self_addr) => match other.addr {
				SocketAddr::V4(other_addr) => {
					let self_port = self_addr.port();
					let other_port = other_addr.port();
					if self_port == other_port {
						self_addr.ip().cmp(other_addr.ip())
					}
					else {
						self_port.cmp(&other_port)
					}
				},
				SocketAddr::V6(_) => Ordering::Less,
			},
			SocketAddr::V6(self_addr) => match other.addr {
				SocketAddr::V4(_) => Ordering::Greater,
				SocketAddr::V6(other_addr) => {
					let self_port = self_addr.port();
					let other_port = other_addr.port();
					if self_port == other_port {
						self_addr.ip().cmp(other_addr.ip())
					}
					else {
						self_port.cmp(&other_port)
					}
				},
			},
		}
	}
}

impl PartialOrd for Node {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		Some(self.cmp(other))
	}
}

#[derive(Debug)]
pub enum NodeTableError { AddressAlreadyAdded, NoAddressInTable }

#[derive(Default, Debug)]
pub struct NodeTable<T = RealTime> where T: Time {
	/// Time source.
	time: T,
	/// Preferable services.
	preferable_services: Services,
	/// Nodes by socket address.
	by_addr: HashMap<SocketAddr, Node>,
	/// Nodes sorted by score.
	by_score: BTreeSet<NodeByScore>,
	/// Nodes sorted by time.
	by_time: BTreeSet<NodeByTime>,
}

impl NodeTable {
	#[cfg(test)]
	/// Creates empty node table with preferable services.
	pub fn new(preferable_services: Services) -> Self {
		NodeTable {
			preferable_services,
			..Default::default()
		}
	}

	/// Opens a file loads node_table from it.
	pub fn from_file<P>(preferable_services: Services, path: P) -> Result<Self, io::Error> where P: AsRef<path::Path> {
		fs::OpenOptions::new()
			.create(true)
			.read(true)
			// without opening for write, mac os returns os error 22
			.write(true)
			.open(path)
			.and_then(|f| Self::load(preferable_services, f))
	}

	/// Saves node table to file
	pub fn save_to_file<P>(&self, path: P) -> Result<(), io::Error> where P: AsRef<path::Path> {
		fs::File::create(path).and_then(|file| self.save(file))
	}
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
					is_preferable: services.includes(&self.preferable_services),
					failures: 0,
				};
				self.by_score.insert(node.clone().into());
				self.by_time.insert(node.clone().into());
				entry.insert(node);
			}
		}
	}

	pub fn exists(&self, addr: SocketAddr) -> bool {
		self.by_addr.contains_key(&addr)
	}

	pub fn add(&mut self, addr: SocketAddr, services: Services) -> Result<(), NodeTableError> {
		if self.exists(addr.clone()) {
			Err(NodeTableError::AddressAlreadyAdded)
		}
		else {
			self.insert(addr, services);
			Ok(())
		}
	}

	/// Tries to remove node with the speicified socket address
	/// from table, if exists.
	/// Returnes `true` if it has removed anything
	pub fn remove(&mut self, addr: &SocketAddr) -> Result<(), NodeTableError> {
		let node = self.by_addr.remove(&addr);
		match node {
			Some(val) => {
				self.by_time.remove(&val.clone().into());
				self.by_score.remove(&val.into());
				Ok(())
			}
			None => Err(NodeTableError::NoAddressInTable)
		}
	}

	/// Inserts many new addresses into node table.
	/// Used in `addr` request handler.
	/// Discards all nodes with timestamp newer than current time.
	pub fn insert_many(&mut self, addresses: Vec<AddressEntry>) {
		// discard all nodes with timestamp newer than current time.
		let now = self.time.get().sec;
		let iter = addresses.into_iter()
			.filter(|addr| addr.timestamp as i64 <= now);

		// iterate over the rest
		for addr in iter {
			let node = Node {
				addr: SocketAddr::new(addr.address.address.into(), addr.address.port.into()),
				time: addr.timestamp as i64,
				services: addr.address.services,
				is_preferable: addr.address.services.includes(&self.preferable_services),
				failures: 0,
			};

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
	pub fn nodes_with_services(&self, services: &Services, protocol: InternetProtocol, except: &HashSet<net::SocketAddr>, limit: usize) -> Vec<Node> {
		self.by_score.iter()
			.filter(|node| protocol.is_allowed(&node.0.addr))
			.filter(|node| node.0.services.includes(services))
			.filter(|node| {
				let node_address = node.0.address();
				!except.contains(&node_address)
					&& match node_address {
						net::SocketAddr::V4(v4) => !except
							.contains(&net::SocketAddr::V6(net::SocketAddrV6::new(v4.ip().to_ipv6_compatible(), v4.port(), 0, 0))),
						net::SocketAddr::V6(v6) => v6.ip().to_ipv4()
							.map(|v4| !except.contains(&net::SocketAddr::V4(net::SocketAddrV4::new(v4, v6.port()))))
							.unwrap_or(true),
					}
			})
			.map(|node| node.0.clone())
			.take(limit)
			.collect()
	}

	/// Returnes all nodes
	pub fn nodes(&self) -> Vec<Node> {
		self.by_addr.iter().map(|(_, n)| n).cloned().collect()
	}

	/// Returns most recently active nodes.
	///
	/// The documenation says:
	/// "Non-advertised nodes should be forgotten after typically 3 hours"
	/// but bitcoin client still advertises them even after a month.
	/// Let's do the same.
	///
	/// https://en.bitcoin.it/wiki/Protocol_documentation#addr
	pub fn recently_active_nodes(&self, protocol: InternetProtocol) -> Vec<Node> {
		self.by_time.iter()
			.filter(|node| protocol.is_allowed(&node.0.addr))
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

	/// Save node table in csv format.
	pub fn save<W>(&self, write: W) -> Result<(), io::Error> where W: io::Write {
		let mut writer = csv::WriterBuilder::new()
			.delimiter(b' ')
			.from_writer(write);
		let iter = self.by_score.iter()
			.map(|node| &node.0)
			.take(1000);

		let err = || io::Error::new(io::ErrorKind::Other, "Write csv error");

		for n in iter {
			let record = (n.addr.to_string(), n.time, u64::from(n.services), n.failures);
			try!(writer.serialize(record).map_err(|_| err()));
		}

		Ok(())
	}

	/// Loads table in from a csv source.
	pub fn load<R>(preferable_services: Services, read: R) -> Result<Self, io::Error> where R: io::Read, T: Default {
		let mut rdr = csv::ReaderBuilder::new()
			.has_headers(false)
			.delimiter(b' ')
			.from_reader(read);

		let mut node_table = NodeTable::default();
		node_table.preferable_services = preferable_services;

		let err = || io::Error::new(io::ErrorKind::Other, "Load csv error");

		for row in rdr.deserialize() {
			let (addr, time, services, failures): (String, i64, u64, u32) = try!(row.map_err(|_| err()));

			let services = services.into();
			let node = Node {
				addr: try!(addr.parse().map_err(|_| err())),
				time: time,
				services: services,
				is_preferable: services.includes(&preferable_services),
				failures: failures,
			};

			node_table.by_score.insert(node.clone().into());
			node_table.by_time.insert(node.clone().into());
			node_table.by_addr.insert(node.addr, node);
		}

		Ok(node_table)
	}
}

#[cfg(test)]
mod tests {
	use std::net::SocketAddr;
	use std::collections::HashSet;
	use message::common::Services;
	use util::InternetProtocol;
	use util::time::{IncrementalTime, ZeroTime};
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
		let nodes = table.nodes_with_services(&Services::default(), InternetProtocol::default(), &HashSet::new(), 2);
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
		let nodes = table.nodes_with_services(&Services::default(), InternetProtocol::default(), &HashSet::new(), 10);
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

		let nodes = table.recently_active_nodes(InternetProtocol::default());
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

	#[test]
	fn test_node_table_duplicates() {
		let s0: SocketAddr = "127.0.0.1:8000".parse().unwrap();
		let s1: SocketAddr = "127.0.0.1:8001".parse().unwrap();
		let mut table = NodeTable::<ZeroTime>::default();
		table.insert(s0, Services::default());
		table.insert(s1, Services::default());
		table.note_failure(&s0);
		table.note_failure(&s1);
	}

	#[test]
	fn add_node() {
		let mut table = NodeTable::<ZeroTime>::default();
		let add_result = table.add("127.0.0.1:8001".parse().unwrap(), Services::default());

		assert!(add_result.is_ok())
	}

	#[test]
	fn add_duplicate() {
		let mut table = NodeTable::<ZeroTime>::default();
		table.add("127.0.0.1:8001".parse().unwrap(), Services::default()).unwrap();
		let add_result = table.add("127.0.0.1:8001".parse().unwrap(), Services::default());

		assert!(add_result.is_err())
	}

	#[test]
	fn remove() {
		let mut table = NodeTable::<ZeroTime>::default();
		table.add("127.0.0.1:8001".parse().unwrap(), Services::default()).unwrap();
		let remove_result = table.remove(&"127.0.0.1:8001".parse().unwrap());

		assert!(remove_result.is_ok());
		assert_eq!(0, table.by_addr.len());
		assert_eq!(0, table.by_score.len());
		assert_eq!(0, table.by_time.len());
	}

	#[test]
	fn remove_nonexistant() {
		let mut table = NodeTable::<ZeroTime>::default();
		let remove_result = table.remove(&"127.0.0.1:8001".parse().unwrap());

		assert!(remove_result.is_err());
	}

	#[test]
	fn test_save_and_load() {
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

		let mut db = Vec::new();
		assert_eq!(table.save(&mut db).unwrap(), ());
		let loaded_table = NodeTable::<IncrementalTime>::load(Services::default(), &db as &[u8]).unwrap();
		assert_eq!(table.by_addr, loaded_table.by_addr);
		assert_eq!(table.by_score, loaded_table.by_score);
		assert_eq!(table.by_time, loaded_table.by_time);

		let s = String::from_utf8(db).unwrap();
		assert_eq!(
"127.0.0.1:8001 7 0 0
127.0.0.1:8004 6 0 0
127.0.0.1:8000 0 0 0
127.0.0.1:8002 5 0 1
127.0.0.1:8003 3 0 1
".to_string(), s);
	}

	#[test]
	fn test_preferable_services() {
		let s0: SocketAddr = "127.0.0.1:8000".parse().unwrap();
		let s1: SocketAddr = "127.0.0.1:8001".parse().unwrap();

		let mut table = NodeTable::new(Services::default().with_network(true).with_bitcoin_cash(true));
		table.insert(s0, Services::default().with_network(true));
		table.insert(s1, Services::default().with_network(true).with_bitcoin_cash(true));
		assert_eq!(table.nodes_with_services(&Services::default(), InternetProtocol::default(), &HashSet::new(), 1)[0].address(), s1);

		table.note_failure(&s1);
		assert_eq!(table.nodes_with_services(&Services::default(), InternetProtocol::default(), &HashSet::new(), 1)[0].address(), s0);

		table.note_failure(&s0);
		assert_eq!(table.nodes_with_services(&Services::default(), InternetProtocol::default(), &HashSet::new(), 1)[0].address(), s1);
	}
}
