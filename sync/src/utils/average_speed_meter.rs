use std::collections::VecDeque;
use time;

/// Speed meter with given items number
#[derive(Debug, Default)]
pub struct AverageSpeedMeter {
	/// Number of items to inspect
	inspect_items: usize,
	/// Number of items currently inspected
	inspected_items: VecDeque<f64>,
	/// Current speed
	speed: f64,
	/// Last timestamp
	last_timestamp: Option<f64>,
}

impl AverageSpeedMeter {
	pub fn with_inspect_items(inspect_items: usize) -> Self {
		assert!(inspect_items > 0);
		AverageSpeedMeter {
			inspect_items: inspect_items,
			inspected_items: VecDeque::with_capacity(inspect_items),
			speed: 0_f64,
			last_timestamp: None,
		}
	}

	pub fn speed(&self) -> f64 {
		let items_per_second = 1_f64 / self.speed;
		if items_per_second.is_normal() { items_per_second } else { 0_f64 }
	}

	pub fn inspected_items_len(&self) -> usize {
		self.inspected_items.len()
	}

	pub fn checkpoint(&mut self) {
		// if inspected_items is already full => remove oldest item from average
		if self.inspected_items.len() == self.inspect_items {
			let oldest = self.inspected_items.pop_front().expect("len() is not zero; qed");
			self.speed = (self.inspect_items as f64 * self.speed - oldest) / (self.inspect_items as f64 - 1_f64);
		}

		// add new item
		let now = time::precise_time_s();
		if let Some(last_timestamp) = self.last_timestamp {
			let newest = now - last_timestamp;
			self.speed = (self.inspected_items.len() as f64 * self.speed + newest) / (self.inspected_items.len() as f64 + 1_f64);
			self.inspected_items.push_back(newest);
		}
		self.last_timestamp = Some(now);
	}

	pub fn start(&mut self) {
		self.last_timestamp = Some(time::precise_time_s());
	}

	pub fn stop(&mut self) {
		self.last_timestamp = None;
	}
}
