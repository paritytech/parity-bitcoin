extern crate log;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate jsonrpc_core;
#[macro_use]
extern crate jsonrpc_macros;
extern crate jsonrpc_http_server;
extern crate tokio_core;
extern crate sync;
extern crate chain;
extern crate serialization as ser;
extern crate primitives;
extern crate p2p;
extern crate network;
extern crate db;
extern crate miner;
extern crate verification;
extern crate script as global_script;
extern crate keys;

pub mod v1;
pub mod rpc_server;

pub use rustc_serialize::hex;

pub use jsonrpc_core::{MetaIoHandler, Compatibility};
pub use jsonrpc_http_server::tokio_core::reactor::{Remote};

pub use jsonrpc_http_server::{Server, Error};
pub use rpc_server::start_http;
