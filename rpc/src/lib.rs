#![cfg_attr(feature="nightly", plugin(serde_macros))]

#[macro_use]
extern crate log;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate jsonrpc_core;
extern crate jsonrpc_http_server;

pub mod v1;
pub mod rpc_server;

pub use jsonrpc_http_server::{Server, RpcServerError};
pub use self::rpc_server::{RpcServer, Extendable};
