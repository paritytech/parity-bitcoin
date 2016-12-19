use message::types;

/// Connection fee rate filter
#[derive(Debug, Default)]
pub struct FeeRateFilter {
	/// Minimal fee in satoshis per 1000 bytes
	fee_rate: u64,
}

impl FeeRateFilter {
	/// Set minimal fee rate, this filter accepts
	pub fn set_min_fee_rate(&mut self, message: types::FeeFilter) {
		self.fee_rate = message.fee_rate;
	}

	/// Filter transaction using its fee rate
	pub fn filter_transaction(&self, tx_fee_rate: Option<u64>) -> bool {
		tx_fee_rate
			.map(|tx_fee_rate| tx_fee_rate >= self.fee_rate)
			.unwrap_or(true)
	}
}

#[cfg(test)]
mod tests {
	use message::types;
	use super::FeeRateFilter;

	#[test]
	fn fee_rate_filter_empty() {
		assert!(FeeRateFilter::default().filter_transaction(Some(0)));
		assert!(FeeRateFilter::default().filter_transaction(None));
	}

	#[test]
	fn fee_rate_filter_accepts() {
		let mut filter = FeeRateFilter::default();
		filter.set_min_fee_rate(types::FeeFilter::with_fee_rate(1000));
		assert!(filter.filter_transaction(Some(1000)));
		assert!(filter.filter_transaction(Some(2000)));
	}

	#[test]
	fn fee_rate_filter_rejects() {
		let mut filter = FeeRateFilter::default();
		filter.set_min_fee_rate(types::FeeFilter::with_fee_rate(1000));
		assert!(!filter.filter_transaction(Some(500)));
	}
}
