use futures::{empty, Empty};
use tokio_core::reactor::Core;

pub fn event_loop() -> Core {
	Core::new().unwrap()
}

pub fn forever() -> Empty<(), ()> {
	empty()
}
