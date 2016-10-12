use net::{Connections, Subscriber};

pub struct P2P {
	connections: Connections,
	subscriber: Subscriber,
}

impl P2P {
	pub fn new() -> Self {
		P2P {
			connections: Connections::default(),
			subscriber: Subscriber::default(),
		}
	}
}
