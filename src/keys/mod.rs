//! Bitcoin keys.
//!
//! `Secret` - 32 bytes
//! `Public` - 65 bytes (TODO: make it optionally compressed)
//! `Private` - secret with additional network identifier (and compressed flag?)
//! `AddressHash` - 20 bytes derived from public
//! `Address` - address_hash with network identifier and format type

mod address;
mod checksum;
pub mod display;
pub mod generator;
pub mod keypair;
mod error;
mod private;
mod public;
mod signature;

pub use self::address::{Type, Address};
pub use self::checksum::checksum;
pub use self::display::DisplayLayout;
pub use self::keypair::KeyPair;
pub use self::error::Error;
pub use self::private::Private;
pub use self::public::Public;
pub use self::signature::{Signature, CompactSignature};

use hash::{H160, H256};
pub type AddressHash = H160;
pub type Secret = H256;
pub type Message = H256;

use secp256k1;
lazy_static! {
	pub static ref SECP256K1: secp256k1::Secp256k1 = secp256k1::Secp256k1::new();
}

