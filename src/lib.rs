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

extern crate rand;
extern crate byteorder;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;
extern crate secp256k1;
extern crate base58;

extern crate bitcrypto as crypto;
extern crate chain;
extern crate primitives;
extern crate serialization as ser;

pub mod keys;
pub mod net;
pub mod script;

pub mod network;

pub use rustc_serialize::hex;
pub use primitives::{hash, bytes};
