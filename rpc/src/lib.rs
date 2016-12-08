#![cfg_attr(feature="nightly", plugin(serde_macros))]

#[macro_use]
extern crate log;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate jsonrpc_core;
extern crate jsonrpc_http_server;
extern crate tokio_core;
extern crate sync;
extern crate chain;
extern crate serialization as ser;
extern crate primitives;
extern crate p2p;
extern crate network;
extern crate db;
extern crate test_data;

pub mod v1;
pub mod rpc_server;

pub use jsonrpc_http_server::{Server, RpcServerError};
pub use self::rpc_server::{RpcServer, Extendable};
