use std::time::Instant;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use util::interval::{Interval, RealInterval};

use message::{Command, Payload};
use message::types::{Ping, Pong};

// delay somewhere near communication timeout
const ENORMOUS_PING_DELAY: f64 = 10f64;

#[derive(Default, Clone, Debug)]
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

	pub fn val(&self) -> u64 {
		self.bytes
	}
}

pub enum Flow { Receive, Send }

#[derive(Default, Clone)]
pub struct PeerStats<T: Interval = RealInterval> {
	pub last_send: u32,
	pub last_recv: u32,

	pub total_send: u64,
	pub total_recv: u64,

	pub avg_ping: f64,
	pub min_ping: Option<f64>,

	send_avg: HashMap<Command, RunningAverage>,
	recv_avg: HashMap<Command, RunningAverage>,

	last_ping: Option<Instant>,
	ping_count: u64,

	interval: T,
}

impl<I: Interval> PeerStats<I> {

	pub fn with_interval(interval: I) -> PeerStats<I> {
		PeerStats {
			interval: interval,
			.. PeerStats::default()
		}
	}

	pub fn report_send(&mut self, command: Command, bytes: usize) {
		self.total_send += bytes as u64;
		self.last_send = ::time::get_time().sec as u32;

		if command == Ping::command() {
			self.report_ping_send();
		}

		match self.send_avg.entry(command) {
			Entry::Occupied(mut avg) => {
				avg.get_mut().add(bytes);
			},
			Entry::Vacant(entry) => {
				entry.insert(RunningAverage::new(bytes));
			},
		}
	}

	fn report_ping_send(&mut self) {
		self.last_ping = Some(self.interval.now());
		self.ping_count += 1;
	}

	fn report_pong_recv(&mut self) {
		if let Some(last_ping) = self.last_ping {
			let dur = self.interval.elapsed(last_ping);
			let update = if dur.as_secs() > 10 {
				ENORMOUS_PING_DELAY
			}
			else {
				// max is 10, checked above, dur.as_secs() as u32 cannot overflow; qed
				f64::from(dur.as_secs() as u32) + f64::from(dur.subsec_nanos()) / 1e9
			};
			self.min_ping = Some(self.min_ping.unwrap_or(ENORMOUS_PING_DELAY).min(update));
			self.avg_ping += (update - self.avg_ping) / (self.ping_count as f64);
		}
	}

	pub fn report_recv(&mut self, command: Command, bytes: usize) {
		self.total_recv += bytes as u64;
		self.last_recv = ::time::get_time().sec as u32;

		if command == Pong::command() {
			self.report_pong_recv();
		}

		match self.recv_avg.entry(command) {
			Entry::Occupied(mut avg) => {
				avg.get_mut().add(bytes);
			},
			Entry::Vacant(entry) => {
				entry.insert(RunningAverage::new(bytes));
			},
		}
	}

	pub fn avg<T>(&self, dir: Flow, cmd: T) -> u64
		where T: Into<Command>
	{
		match dir {
			Flow::Receive => self.recv_avg.get(&cmd.into()).and_then(|x| Some(x.val())).unwrap_or_default(),
			Flow::Send => self.send_avg.get(&cmd.into()).and_then(|x| Some(x.val())).unwrap_or_default(),
		}
	}
}

#[cfg(test)]
mod tests {

	use super::{RunningAverage, PeerStats, Flow};
	use util::interval::{FixedIntervalSpawner, RealInterval};

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

	#[test]
	fn smoky() {
		let mut stats = PeerStats::<FixedIntervalSpawner>::with_interval(FixedIntervalSpawner::new(50));
		stats.report_send("ping".into(), 200);

		assert_eq!(stats.send_avg[&"ping".into()].val(), 200);

		stats.report_recv("pong".into(), 50);
		assert!(stats.avg_ping > 0.03);
		assert!(stats.avg_ping < 0.1);
	}

	#[test]
	fn avg_t() {
		let mut stats = PeerStats::<RealInterval>::default();
		stats.report_send("inv".into(), 200);
		stats.report_send("inv".into(), 300);

		assert_eq!(stats.avg(Flow::Send, "inv"), 250);

		let mut stats = PeerStats::<RealInterval>::default();
		stats.report_recv("inv".into(), 2000);
		stats.report_recv("inv".into(), 3000);

		assert_eq!(stats.avg(Flow::Receive, "inv"), 2500);
	}
}
