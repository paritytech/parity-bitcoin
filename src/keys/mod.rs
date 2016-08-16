pub mod generator;
pub mod keypair;
mod error;

use secp256k1;
use hash::{H256, H512};
pub use self::keypair::KeyPair;
pub use self::error::Error;

pub type Secret = H256;
pub type Public = H512;

lazy_static! {
	pub static ref SECP256K1: secp256k1::Secp256k1 = secp256k1::Secp256k1::new();
}

