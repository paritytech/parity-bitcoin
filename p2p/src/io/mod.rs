
macro_rules! try_async {
	($e: expr) => (match try_nb!($e) {
		Async::Ready(i) => i,
		Async::NotReady => return Ok(Async::NotReady),
	})
}

mod error;
mod handshake;
mod read_header;
mod read_message;
mod read_payload;
mod write_message;

pub use self::error::Error;
pub use self::handshake::{handshake, Handshake};
pub use self::read_header::{read_header, ReadHeader};
pub use self::read_message::{read_message, ReadMessage};
pub use self::read_payload::{read_payload, ReadPayload};
pub use self::write_message::{write_message, WriteMessage};
