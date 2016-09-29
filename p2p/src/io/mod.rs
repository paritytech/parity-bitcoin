mod error;
mod handshake;
mod read_header;
mod read_message;
mod read_payload;
mod write_message;

pub const VERSION: u32 = 70_000;
pub const USER_AGENT: &'static str = "pbtc";

pub use self::error::Error;
pub use self::handshake::{handshake, accept_handshake, Handshake, AcceptHandshake};
pub use self::read_header::{read_header, ReadHeader};
pub use self::read_message::{read_message, ReadMessage};
pub use self::read_payload::{read_payload, ReadPayload};
pub use self::write_message::{write_message, WriteMessage};
