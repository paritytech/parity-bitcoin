use futures::{oneshot, Oneshot};
use message::Payload;
use net::Connection;

pub struct Connections {
	channels: Vec<Connection>,
}

impl Connections {
	/// Broadcast messages to the network.
	/// Returned future completes of first confirmed receive.
	pub fn broadcast(&self, payload: Payload) -> Oneshot<Payload> {
		let (complete, os) = oneshot::<Payload>();
		let mut complete = Some(complete);

		for channel in &self.channels {
			//channel.write_message(
		}

		os
	}
}
