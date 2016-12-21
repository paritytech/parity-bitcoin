use message::Command;
use std::collections::hash_map::Entry;
use std::collections::HashMap;

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
		// let x = self.bytes
		// let y = bytes, y >= 0
		// to not overflow, this following be true:
		// x + (y - x) / c >= 0
		// so
		// y / c  >= 0
		// which is true by usize definition;
		// qed
		self.bytes = (self.bytes as i64 + ((bytes as i64 - self.bytes as i64) / self.count as i64)) as u64;
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
		self.total_send += bytes as u64;
		self.last_send = ::time::get_time().sec as u32;
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
		self.total_recv += bytes as u64;
		self.last_recv = ::time::get_time().sec as u32;
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

#[cfg(test)]
mod tests {

	use super::RunningAverage;

	#[test]
	fn avg() {
		let mut avg = RunningAverage::new(10);
		avg.add(12);

		assert_eq!(avg.bytes, 11);
	}

	#[test]
	fn avg_l() {
		let mut avg = RunningAverage::new(10);
		avg.add(12);
		avg.add(20);
		avg.add(28);
		avg.add(12);

		assert_eq!(avg.bytes, 16);
	}
}
