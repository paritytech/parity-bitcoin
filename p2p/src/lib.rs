#[macro_use]
extern crate futures;
extern crate rand;
extern crate time;
extern crate tokio_core;

extern crate bitcrypto as crypto;
extern crate message;
extern crate primitives;
extern crate serialization as ser;

mod error;
pub mod io;
pub mod net;
pub mod tcp;
pub mod util;

pub const VERSION: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";

pub use primitives::bytes;

pub use error::Error;

