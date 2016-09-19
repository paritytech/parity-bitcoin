//! Ethcore's bitcoin library.
//!
//! module(depends_on..)
//!
//! net(primitives, serialization)
//! script(primitives, serialization, chain)
//! chain(primitives, serialization)
//! keys(primitives, crypto)
//! crypto(primitives)
//! serialization(primitives)
//! primitives

extern crate rand;
extern crate byteorder;
extern crate crypto as rcrypto;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;
extern crate secp256k1;
extern crate base58;

pub mod chain;
pub mod keys;
pub mod net;
pub mod primitives;
pub mod script;
pub mod ser;

pub mod crypto;
pub mod network;

pub use rustc_serialize::hex;
pub use primitives::{hash, bytes};
