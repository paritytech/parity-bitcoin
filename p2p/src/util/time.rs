use time;

pub trait Time {
	fn get(&self) -> time::Timespec;
}

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
