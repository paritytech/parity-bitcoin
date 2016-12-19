use message::types;

/// Short info for message
pub trait MessageShortInfo {
	/// Return short info on message
	fn short_info() -> String;
}

/// Long info for message
pub trait MessageLongInfo {
	/// Return long info on message
	fn long_info() -> String;
}

impl MessageShortInfo for types::Inv {
	fn short_info(&self) -> String {
		format!("LEN={}", self.inventory.len())
	}
}

impl MessageLongInfo for types::Inv {
	fn long_info(&self) -> String {
		self.short_info()
	}
}