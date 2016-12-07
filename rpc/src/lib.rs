#![cfg_attr(feature="nightly", plugin(serde_macros))]

#[macro_use]
extern crate log;
extern crate rustc_serialize;
extern crate serde;
extern crate serde_json;
extern crate jsonrpc_core;

pub mod v1;
