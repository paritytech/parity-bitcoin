use futures::{oneshot, Oneshot};
use message::PayloadType;
use net::Connection;

pub struct Connections {
	channels: Vec<Connection>,
}

impl Connections {
	/// Broadcast messages to the network.
	/// Returned future completes of first confirmed receive.
	pub fn broadcast<T>(&self, payload: T) -> Oneshot<()> where T: PayloadType {
		let (complete, os) = oneshot::<()>();
		let mut complete = Some(complete);

		for channel in &self.channels {
			//channel.write_message(
		}

		os
	}
}
