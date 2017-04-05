use std::time::{Instant, Duration};

pub trait Interval : Default {
	fn now(&self) -> Instant {
		Instant::now()
	}

	fn elapsed(&self, instant: Instant) -> Duration {
		instant.elapsed()
	}
}

#[derive(Default)]
pub struct RealInterval;

impl Interval for RealInterval { }

#[derive(Default)]
#[cfg(test)]
pub struct FixedIntervalSpawner {
	step_millis: u64,
}

#[cfg(test)]
impl FixedIntervalSpawner {
	pub fn new(step_millis: u64) -> Self {
		FixedIntervalSpawner { step_millis : step_millis }
	}
}

#[cfg(test)]
impl Interval for FixedIntervalSpawner {
	fn now(&self) -> Instant {
		Instant::now()
	}

	fn elapsed(&self, instant: Instant) -> Duration {
		(instant - Duration::from_millis(self.step_millis)).elapsed()
	}
}
