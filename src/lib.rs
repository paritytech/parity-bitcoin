//! Ethcore's bitcoin library.
//!
//! module(depends_on..)
//!
//! net(primitives, serialization)
//! script(primitives, serialization, chain, crypto)
//! chain(primitives, serialization, crypto)
//! keys(primitives, crypto)
//! crypto(primitives)
//! serialization(primitives)
//! primitives

extern crate byteorder;
extern crate rustc_serialize;

extern crate bitcrypto as crypto;
extern crate chain;
extern crate keys;
extern crate primitives;
extern crate script;
extern crate serialization as ser;

pub mod net;

pub use rustc_serialize::hex;
pub use primitives::{hash, bytes};
