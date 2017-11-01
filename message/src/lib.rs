extern crate byteorder;
extern crate bitcrypto as crypto;
extern crate chain;
extern crate primitives;
extern crate serialization as ser;
#[macro_use]
extern crate serialization_derive;
extern crate network;

pub mod common;
mod message;
mod serialization;
pub mod types;
mod error;

pub use primitives::{hash, bytes};

pub use common::{Command, Services};
pub use message::{Message, MessageHeader, Payload, to_raw_message};
pub use serialization::{serialize_payload, deserialize_payload};
pub use error::{Error, MessageResult};
