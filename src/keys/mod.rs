pub mod generator;
pub mod keypair;
mod error;

use secp256k1;
use hash::{H160, H256, H520};
pub use self::keypair::KeyPair;
pub use self::error::Error;

pub type Address = H160;
pub type Secret = H256;
pub type Public = H520;

lazy_static! {
	pub static ref SECP256K1: secp256k1::Secp256k1 = secp256k1::Secp256k1::new();
}

