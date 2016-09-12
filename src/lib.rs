//! Ethcore's bitcoin library.

extern crate rand;
extern crate byteorder;
extern crate crypto as rcrypto;
extern crate rustc_serialize;
#[macro_use]
extern crate lazy_static;
extern crate secp256k1;
extern crate base58;

pub mod keys;
pub mod script;
pub mod block;
pub mod block_header;
pub mod compact_integer;
pub mod crypto;
pub mod hash;
pub mod merkle_root;
pub mod network;
pub mod reader;
pub mod sign;
pub mod stream;
pub mod transaction;

pub use rustc_serialize::hex;
