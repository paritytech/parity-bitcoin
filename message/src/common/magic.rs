//! Bitcoin network magic number
//! https://www.anintegratedworld.com/unravelling-the-mysterious-block-chain-magic-number/

use ser::{Stream, Serializable};
use Error;

const MAGIC_MAINNET: u32 = 0xD9B4BEF9;
const MAGIC_TESTNET: u32 = 0x0709110B;

/// Bitcoin network
/// https://bitcoin.org/en/glossary/mainnet
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Magic {
	/// The original and main network for Bitcoin transactions, where satoshis have real economic value.
	Mainnet,
	Testnet,
}

impl From<Magic> for u32 {
	fn from(m: Magic) -> Self {
		match m {
			Magic::Mainnet => MAGIC_MAINNET,
			Magic::Testnet => MAGIC_TESTNET,
		}
	}
}

impl Magic {
	pub fn from_u32(magic: u32) -> Result<Self, Error> {
		match magic {
			MAGIC_MAINNET => Ok(Magic::Mainnet),
			MAGIC_TESTNET => Ok(Magic::Testnet),
			_ => Err(Error::InvalidMagic),
		}
	}

	pub fn port(&self) -> u16 {
		match *self {
			Magic::Mainnet => 8333,
			Magic::Testnet => 18333,
		}
	}

	pub fn rpc_port(&self) -> u16 {
		match *self {
			Magic::Mainnet => 8332,
			Magic::Testnet => 18332,
		}
	}
}

impl Serializable for Magic {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u32::from(*self));
	}
}

#[cfg(test)]
mod tests {
	use Error;
	use super::{Magic, MAGIC_MAINNET, MAGIC_TESTNET};

	#[test]
	fn test_network_magic_number() {
		assert_eq!(MAGIC_MAINNET, Magic::Mainnet.into());
		assert_eq!(MAGIC_TESTNET, Magic::Testnet.into());
		assert_eq!(Magic::from_u32(MAGIC_MAINNET).unwrap(), Magic::Mainnet);
		assert_eq!(Magic::from_u32(MAGIC_TESTNET).unwrap(), Magic::Testnet);
		assert_eq!(Magic::from_u32(0).unwrap_err(), Error::InvalidMagic);
	}

	#[test]
	fn test_network_port() {
		assert_eq!(Magic::Mainnet.port(), 8333);
		assert_eq!(Magic::Testnet.port(), 18333);
	}

	#[test]
	fn test_network_rpc_port() {
		assert_eq!(Magic::Mainnet.rpc_port(), 8332);
		assert_eq!(Magic::Testnet.rpc_port(), 18332);
	}
}
