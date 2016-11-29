use std::cmp;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counts number of open inbound and outbound connections.
pub struct ConnectionCounter {
	/// Current number of inbound connections.
	current_inbound_connections: AtomicUsize,
	/// Current number of outbound connections.
	current_outbound_connections: AtomicUsize,
	/// Maximum number of inbound connections.
	max_inbound_connections: u32,
	/// Maximum number of outbound connections.
	max_outbound_connections: u32,
}

impl ConnectionCounter {
	pub fn new(max_inbound_connections: u32, max_outbound_connections: u32) -> Self {
		ConnectionCounter {
			current_inbound_connections: AtomicUsize::new(0),
			current_outbound_connections: AtomicUsize::new(0),
			max_inbound_connections: max_inbound_connections,
			max_outbound_connections: max_outbound_connections,
		}
	}

	/// Increases inbound connections counter by 1.
	pub fn note_new_inbound_connection(&self) {
		self.current_inbound_connections.fetch_add(1, Ordering::AcqRel);
	}

	/// Decreases inbound connections counter by 1.
	/// If it underflows, it means, that there is a logic error.
	pub fn note_close_inbound_connection(&self) {
		self.current_inbound_connections.fetch_sub(1, Ordering::AcqRel);
	}

	/// Increases outbound connections counter by 1.
	pub fn note_new_outbound_connection(&self) {
		self.current_outbound_connections.fetch_add(1, Ordering::AcqRel);
	}

	/// Decreases outbound connections counter by 1.
	/// If it underflows, it means, that there is a logic error.
	pub fn note_close_outbound_connection(&self) {
		self.current_outbound_connections.fetch_sub(1, Ordering::AcqRel);
	}

	/// Returns number of inbound connections needed to reach the maximum
	pub fn inbound_connections_needed(&self) -> u32 {
		let ic = self.inbound_connections();
		ic.1 - cmp::min(ic.0, ic.1)
	}

	/// Returns number of inbound connections needed to reach the maximum
	pub fn outbound_connections_needed(&self) -> u32 {
		let oc = self.outbound_connections();
		oc.1 - cmp::min(oc.0, oc.1)
	}

	/// Returns a pair of unsigned integers where first element is current number of connections and the second is max.
	pub fn inbound_connections(&self) -> (u32, u32) {
		let current = self.current_inbound_connections.load(Ordering::Acquire) as u32;
		(current, self.max_inbound_connections)
	}

	/// Returns a pair of unsigned integers where first element is current number of connections and the second is max.
	pub fn outbound_connections(&self) -> (u32, u32) {
		let current = self.current_outbound_connections.load(Ordering::Acquire) as u32;
		(current, self.max_outbound_connections)
	}
}

#[cfg(test)]
mod tests {
	use super::ConnectionCounter;

	#[test]
	fn test_inbound_connection_counter() {
		let cc = ConnectionCounter::new(5, 10);
		assert_eq!(cc.inbound_connections_needed(), 5);
		assert_eq!(cc.inbound_connections(), (0, 5));
		cc.note_new_inbound_connection();
		assert_eq!(cc.inbound_connections_needed(), 4);
		assert_eq!(cc.inbound_connections(), (1, 5));
		cc.note_new_inbound_connection();
		cc.note_new_inbound_connection();
		cc.note_new_inbound_connection();
		cc.note_new_inbound_connection();
		assert_eq!(cc.inbound_connections_needed(), 0);
		// it may exceed max
		cc.note_new_inbound_connection();
		assert_eq!(cc.inbound_connections_needed(), 0);
		assert_eq!(cc.inbound_connections(), (6, 5));
		cc.note_close_inbound_connection();
		assert_eq!(cc.inbound_connections_needed(), 0);
		assert_eq!(cc.inbound_connections(), (5, 5));
	}

	#[test]
	fn test_outbound_connection_counter() {
		let cc = ConnectionCounter::new(0, 4);
		assert_eq!(cc.outbound_connections_needed(), 4);
		assert_eq!(cc.outbound_connections(), (0, 4));
		cc.note_new_outbound_connection();
		cc.note_new_outbound_connection();
		assert_eq!(cc.outbound_connections_needed(), 2);
		assert_eq!(cc.outbound_connections(), (2, 4));
		cc.note_close_outbound_connection();
		assert_eq!(cc.outbound_connections_needed(), 3);
		assert_eq!(cc.outbound_connections(), (1, 4));
	}
}
