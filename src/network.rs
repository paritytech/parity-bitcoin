//! Bitcoin network

const MAGIC_MAINNET: u32 = 0xD9B4BEF9;
const MAGIC_TESTNET: u32 = 0x0709110B;

/// Bitcoin network
/// https://bitcoin.org/en/glossary/mainnet
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Network {
	/// The original and main network for Bitcoin transactions, where satoshis have real economic value.
	Mainnet,
	Testnet,
}

#[derive(Debug, PartialEq)]
pub struct FromMagicError;

impl Network {
	/// Magic number
	/// https://www.anintegratedworld.com/unravelling-the-mysterious-block-chain-magic-number/
	pub fn magic(&self) -> u32 {
		match *self {
			Network::Mainnet => MAGIC_MAINNET,
			Network::Testnet => MAGIC_TESTNET,
		}
	}

	pub fn from_magic(magic: u32) -> Result<Self, FromMagicError> {
		match magic {
			MAGIC_MAINNET => Ok(Network::Mainnet),
			MAGIC_TESTNET => Ok(Network::Testnet),
			_ => Err(FromMagicError),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::{Network, MAGIC_MAINNET, MAGIC_TESTNET, FromMagicError};

	#[test]
	fn test_network_magic_number() {
		assert_eq!(Network::Mainnet.magic(), MAGIC_MAINNET);
		assert_eq!(Network::Testnet.magic(), MAGIC_TESTNET);
		assert_eq!(Network::from_magic(MAGIC_MAINNET).unwrap(), Network::Mainnet);
		assert_eq!(Network::from_magic(MAGIC_TESTNET).unwrap(), Network::Testnet);
		assert_eq!(Network::from_magic(0).unwrap_err(), FromMagicError);
	}
}
