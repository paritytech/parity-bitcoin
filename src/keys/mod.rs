mod address;
mod checksum;
pub mod display;
pub mod generator;
pub mod keypair;
mod error;
mod private;

use secp256k1;
use hash::{H160, H256, H520};
pub use self::address::{Type, Address};
pub use self::checksum::checksum;
pub use self::display::DisplayLayout;
pub use self::keypair::KeyPair;
pub use self::error::Error;
pub use self::private::Private;

pub type AddressHash = H160;
pub type Secret = H256;
pub type Public = H520;

lazy_static! {
	pub static ref SECP256K1: secp256k1::Secp256k1 = secp256k1::Secp256k1::new();
}

