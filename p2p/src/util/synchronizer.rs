#![allow(dead_code)]
/// Number of out of order requests granted permissions by `ConfigurableSynchronizer`.
const CONFIGURABLE_SYNCHRONIZER_THRESHOLD: u32 = 10;

/// Interface for struct responsible for synchronizing responses.
/// First a protocol processing a message, needs to declare that it is willing to send a response
/// by calling `declare_response`. As call result, the protocol will be given a unique response id.
/// Then, once the response is ready, the protocol, should try to get permission for sending the response.
/// If permission is not granted, then sending the response should be rescheduled.
pub trait Synchronizer: Send {
	/// Declare sending response in future.
	fn declare_response(&mut self) -> u32;

	/// Returns true if permission for sending response is granted.
	fn permission_for_response(&mut self, id: u32) -> bool;
}

/// Should be used to send responses for incoming requests in fifo order.
#[derive(Debug, Default)]
pub struct FifoSynchronizer {
	declared_responses: u32,
	next_to_grant: u32,
}

impl Synchronizer for FifoSynchronizer {
	fn declare_response(&mut self) -> u32 {
		let result = self.declared_responses;
		self.declared_responses = self.declared_responses.overflowing_add(1).0;
		result
	}

	fn permission_for_response(&mut self, id: u32) -> bool {
		// there should be an assertion here, assert!(id < self.declared_responses),
		// but it's impossible to write an assertion if the value may overflow
		if id == self.next_to_grant {
			self.next_to_grant = self.next_to_grant.overflowing_add(1).0;
			true
		} else {
			false
		}
	}
}

/// Should be used to send responses for incoming requests asap.
#[derive(Debug, Default)]
pub struct NoopSynchronizer {
	declared_responses: u32,
}

impl Synchronizer for NoopSynchronizer {
	fn declare_response(&mut self) -> u32 {
		let result = self.declared_responses;
		self.declared_responses = self.declared_responses.overflowing_add(1).0;
		result
	}

	fn permission_for_response(&mut self, _id: u32) -> bool {
		true
	}
}

/// Fifo synchronizer which additionally grants permissions to responses within
/// threshold range from start id.
/// Should be used only if we are currently processing requests asynchronously,
/// and are willing to process them synchronously.
#[derive(Debug)]
struct ThresholdSynchronizer {
	inner: FifoSynchronizer,
	start_id: u32,
	threshold: u32,
}

impl ThresholdSynchronizer {
	fn new(next_to_grant: u32, threshold: u32) -> Self {
		// let's mark all ids in threshold as declared
		// this may cause some ids, to be skipped, but we don't care
		// it won't affect correct execution of the program
		let declared = next_to_grant + threshold;
		ThresholdSynchronizer {
			inner: FifoSynchronizer {
				declared_responses: declared,
				next_to_grant: declared,
			},
			start_id: next_to_grant,
			threshold: threshold,
		}
	}
}

impl Synchronizer for ThresholdSynchronizer {
	fn declare_response(&mut self) -> u32 {
		self.inner.declare_response()
	}

	fn permission_for_response(&mut self, id: u32) -> bool {
		if self.inner.permission_for_response(id) {
			return true;
		}

		id.overflowing_sub(self.start_id).0 < self.threshold ||
			self.start_id.overflowing_sub(id).0 < self.threshold
	}
}

#[derive(Debug)]
enum InnerSynchronizer {
	Fifo(FifoSynchronizer),
	Noop(NoopSynchronizer),
	Threshold(ThresholdSynchronizer),
}

impl InnerSynchronizer {
	pub fn new(sync: bool) -> Self {
		if sync {
			InnerSynchronizer::Fifo(FifoSynchronizer::default())
		} else {
			InnerSynchronizer::Noop(NoopSynchronizer::default())
		}
	}
}

#[derive(Debug)]
pub struct ConfigurableSynchronizer {
	/// Inner synchronizer which is currently used
	inner: InnerSynchronizer,
	/// Id of next response which is likely to be granted permission.
	probably_next_to_grant: u32,
}

impl Default for ConfigurableSynchronizer {
	fn default() -> Self {
		ConfigurableSynchronizer::new(false)
	}
}

impl ConfigurableSynchronizer {
	pub fn new(sync: bool) -> Self {
		ConfigurableSynchronizer {
			inner: InnerSynchronizer::new(sync),
			probably_next_to_grant: 0,
		}
	}

	/// Although change happens immediately, responses within CONFIGURABLE_SYNCHRONIZER_THRESHOLD range
	/// from last_processed response will still be granted permissions.
	pub fn change_sync_policy(&mut self, sync: bool) {
		let new_inner = match self.inner {
			InnerSynchronizer::Fifo(ref s) if sync == false => {
				self.probably_next_to_grant = s.next_to_grant;
				InnerSynchronizer::Noop(NoopSynchronizer {
					declared_responses: s.declared_responses,
				})
			},
			InnerSynchronizer::Threshold(ref s) if sync == false => {
				self.probably_next_to_grant = s.inner.next_to_grant;
				InnerSynchronizer::Noop(NoopSynchronizer {
					declared_responses: s.inner.declared_responses,
				})
			},
			InnerSynchronizer::Noop(_) if sync == true => {
				let threshold  = ThresholdSynchronizer::new(
					self.probably_next_to_grant,
					CONFIGURABLE_SYNCHRONIZER_THRESHOLD
				);
				InnerSynchronizer::Threshold(threshold)
			},
			_ => return (),
		};

		self.inner = new_inner;
	}
}

impl Synchronizer for ConfigurableSynchronizer {
	fn declare_response(&mut self) -> u32 {
		match self.inner {
			InnerSynchronizer::Fifo(ref mut s) => s.declare_response(),
			InnerSynchronizer::Noop(ref mut s) => s.declare_response(),
			InnerSynchronizer::Threshold(ref mut s) => s.declare_response(),
		}
	}

	fn permission_for_response(&mut self, id: u32) -> bool {
		match self.inner {
			InnerSynchronizer::Fifo(ref mut s) => s.permission_for_response(id),
			InnerSynchronizer::Threshold(ref mut s) => s.permission_for_response(id),
			InnerSynchronizer::Noop(ref mut s) => {
				self.probably_next_to_grant = id.overflowing_add(1).0;
				s.permission_for_response(id)
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{
		Synchronizer, FifoSynchronizer, NoopSynchronizer, ConfigurableSynchronizer, ThresholdSynchronizer, CONFIGURABLE_SYNCHRONIZER_THRESHOLD
	};

	#[test]
	fn test_fifo_synchronizer() {
		let mut s = FifoSynchronizer::default();
		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(!s.permission_for_response(id1));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));
	}

	#[test]
	fn test_noop_synchronizer() {
		let mut s = NoopSynchronizer::default();
		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));
	}

	#[test]
	fn test_threshold_synchronizer() {
		let mut s = ThresholdSynchronizer::new(2, 2);
		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(!s.permission_for_response(id1));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));
		// historic permissions
		assert!(!s.permission_for_response(0));
		assert!(s.permission_for_response(1));
		assert!(s.permission_for_response(2));
		assert!(s.permission_for_response(3));
		assert!(!s.permission_for_response(4));
	}

	#[test]
	fn test_configurable_synchronizer() {
		let mut s = ConfigurableSynchronizer::new(true);

		// process messages synchronously
		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(!s.permission_for_response(id1));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));

		// process messages asynchronously
		s.change_sync_policy(false);

		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));

		// process messages synchronously again
		s.change_sync_policy(true);

		let last_async = id2;

		// let's check again if we can process them only synchronously
		let id0 = s.declare_response();
		let id1 = s.declare_response();
		let id2 = s.declare_response();
		assert!(!s.permission_for_response(id1));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id0));
		assert!(!s.permission_for_response(id2));
		assert!(s.permission_for_response(id1));
		assert!(s.permission_for_response(id2));


		// there might be ~10 unhandled messages,
		// let's check if we can process them out of order (eg. in reverse)
		for i in (0..CONFIGURABLE_SYNCHRONIZER_THRESHOLD - 1).into_iter().rev() {
			assert!(s.permission_for_response(last_async + i));
		}

		// the next one should fail
		assert!(!s.permission_for_response(last_async + CONFIGURABLE_SYNCHRONIZER_THRESHOLD));
	}
}
