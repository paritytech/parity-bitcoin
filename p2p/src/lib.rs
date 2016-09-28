#[macro_use]
extern crate tokio_core;
extern crate futures;
extern crate primitives;
extern crate serialization as ser;
extern crate net;

macro_rules! try_async {
	($e: expr) => (match try_nb!($e) {
		Async::Ready(i) => i,
		Async::NotReady => return Ok(Async::NotReady),
	})
}

pub mod io;
pub mod stream;

pub use primitives::bytes;
