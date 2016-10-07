extern crate byteorder;
extern crate bitcrypto as crypto;
extern crate chain;
extern crate primitives;
extern crate serialization as ser;

pub mod common;
mod message;
pub mod types;
mod error;

pub use primitives::{hash, bytes};

pub use message::{Message, MessageHeader, Payload};
pub use error::Error;
pub type MessageResult<T> = Result<T, Error>;
