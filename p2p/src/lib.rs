#[macro_use]
extern crate futures;
extern crate futures_cpupool;
extern crate rand;
extern crate time;
extern crate tokio_core;
extern crate parking_lot;
#[macro_use]
extern crate log;

extern crate chain;
extern crate bitcrypto as crypto;
extern crate message;
extern crate primitives;

mod io;
mod net;
mod protocol;
mod session;
mod util;
mod config;
mod event_loop;
mod p2p;

pub const VERSION: u32 = 70_001;
pub const USER_AGENT: &'static str = "pbtc";

pub use primitives::{hash, bytes};

pub use config::Config;
pub use net::Config as NetConfig;
pub use p2p::P2P;
pub use event_loop::{event_loop, forever};
pub use util::{PeerId, PeerInfo};

