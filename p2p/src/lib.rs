#[macro_use]
extern crate futures;
extern crate rand;
extern crate time;
extern crate tokio_core;

extern crate bitcrypto as crypto;
extern crate message;
extern crate primitives;
extern crate serialization as ser;

pub mod io;
pub mod net;
pub mod tcp;
pub mod util;
mod config;
mod error;
mod event_loop;
mod run;

pub const VERSION: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";

pub use primitives::bytes;

pub use config::Config;
pub use error::Error;
pub use event_loop::event_loop;
pub use run::run;

