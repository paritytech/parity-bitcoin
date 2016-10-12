use futures::{oneshot, Oneshot, Future};
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
			// TODO: make is async
			let _wait = channel.write_message(&payload).map(|_message| {
				if let Some(complete) = complete.take() {
					complete.complete(());
				}
			}).map_err(|_err| {
				// reconnect / disconnect
			}).wait();
		}

		os
	}
	//pub fn subscribe<T>(
}
