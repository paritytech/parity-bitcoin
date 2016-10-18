use std::cell::Cell;
use time;

pub trait Time {
	fn get(&self) -> time::Timespec;
}

#[derive(Default)]
pub struct RealTime;

impl Time for RealTime {
	fn get(&self) -> time::Timespec {
		time::get_time()
	}
}

pub struct StaticTime(time::Timespec);

impl StaticTime {
	pub fn new(time: time::Timespec) -> Self {
		StaticTime(time)
	}
}

impl Time for StaticTime {
	fn get(&self) -> time::Timespec {
		self.0
	}
}

#[derive(Default)]
pub struct IncrementalTime {
	counter: Cell<i64>,
}

impl Time for IncrementalTime {
	fn get(&self) -> time::Timespec {
		let c = self.counter.get();
		let result = time::Timespec::new(c, 0);
		self.counter.set(c + 1);
		result
	}
}
