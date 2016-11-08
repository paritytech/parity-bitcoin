#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]

extern crate rustc_serialize;
#[macro_use] extern crate heapsize;

pub mod bytes;
pub mod hash;

pub use rustc_serialize::hex;
