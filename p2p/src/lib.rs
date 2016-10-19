#[macro_use]
extern crate futures;
extern crate futures_cpupool;
extern crate rand;
extern crate time;
extern crate tokio_core;
extern crate parking_lot;

extern crate bitcrypto as crypto;
extern crate message;
extern crate primitives;

pub mod io;
pub mod net;
pub mod protocol;
pub mod session;
pub mod util;
mod config;
mod event_loop;
mod p2p;

pub const VERSION: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";

pub use primitives::{hash, bytes};

pub use config::Config;
pub use event_loop::{event_loop, forever};
pub use p2p::P2P;
pub use util::{PeerId, PeerInfo};

