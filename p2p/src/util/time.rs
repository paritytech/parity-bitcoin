use std::cell::Cell;
use time;

pub trait Time {
	fn get(&self) -> time::Timespec;
}

#[derive(Default, Debug)]
pub struct RealTime;

impl Time for RealTime {
	fn get(&self) -> time::Timespec {
		time::get_time()
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

#[derive(Default)]
pub struct ZeroTime {
}

impl Time for ZeroTime {
	fn get(&self) -> time::Timespec {
		time::Timespec::new(0, 0)
	}
}
