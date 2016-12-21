use message::Command;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

#[derive(Default, Clone)]
pub struct MessageStats {
	pub addr: u64,
	pub getdata: u64,
	pub getheaders: u64,
	pub headers: u64,
	pub reject: u64,
	pub tx: u64,
	pub inv: u64,
	pub ping: u64,
	pub pong: u64,
	pub verack: u64,
	pub version: u64,
}

#[derive(Default, Clone)]
pub struct RunningAverage {
	count: u64,
	bytes: u64,
}

impl RunningAverage {
	fn new(initial: usize) -> Self {
		RunningAverage { count: 1, bytes: initial as u64 }
	}

	fn add(&mut self, bytes: usize) {
		self.count += 1;
		// self.count guaranteed to be at least 1, since self.count min value is 0 and we just added 1 above
		// so division by zero is impossible; qed
		//
		// let x = self.bytes, x >= 0
		// let y = bytes, y >= 0
		// to not overflow, this following be true:
		// x + (y - x) / c >= 0
		// so
		// y / c  >= 0
		// which is true by u64 definition;
		// qed
		self.bytes += (bytes as u64 - self.bytes) / self.count;
	}
}

#[derive(Default, Clone)]
pub struct PeerStats {
	pub last_send: u32,
	pub last_recv: u32,

	pub total_send: u64,
	pub total_recv: u64,

	pub avg_ping: f64,
	pub min_ping: f64,
	pub total_ping: u64,

	pub synced_blocks: u32,
	pub synced_headers: u32,
	pub send_avg: HashMap<Command, RunningAverage>,
	pub recv_avg: HashMap<Command, RunningAverage>,
}

impl PeerStats {
	pub fn report_send(&mut self, command: Command, bytes: usize) {
		match self.send_avg.entry(command) {
			Entry::Occupied(mut avg) => {
				avg.get_mut().add(bytes);
			},
			Entry::Vacant(entry) => {
				entry.insert(RunningAverage::new(bytes));
			},
		}
	}

	pub fn report_recv(&mut self, command: Command, bytes: usize) {
		match self.recv_avg.entry(command) {
			Entry::Occupied(mut avg) => {
				avg.get_mut().add(bytes);
			},
			Entry::Vacant(entry) => {
				entry.insert(RunningAverage::new(bytes));
			},
		}
	}
}
