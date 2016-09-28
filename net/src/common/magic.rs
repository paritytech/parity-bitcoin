//! Bitcoin network magic number
//! https://www.anintegratedworld.com/unravelling-the-mysterious-block-chain-magic-number/

use ser::{Stream, Serializable, Reader, Deserializable, Error as ReaderError};
use common::Error;

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
}

impl Serializable for Magic {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u32::from(*self));
	}
}

impl Deserializable for Magic {
	fn deserialize(reader: &mut Reader) -> Result<Self, ReaderError> where Self: Sized {
		let magic: u32 = try!(reader.read());
		Magic::from_u32(magic).map_err(|_| ReaderError::MalformedData)
	}
}

#[cfg(test)]
mod tests {
	use common::Error;
	use super::{Magic, MAGIC_MAINNET, MAGIC_TESTNET};

	#[test]
	fn test_network_magic_number() {
		assert_eq!(MAGIC_MAINNET, Magic::Mainnet.into());
		assert_eq!(MAGIC_TESTNET, Magic::Testnet.into());
		assert_eq!(Magic::from_u32(MAGIC_MAINNET).unwrap(), Magic::Mainnet);
		assert_eq!(Magic::from_u32(MAGIC_TESTNET).unwrap(), Magic::Testnet);
		assert_eq!(Magic::from_u32(0).unwrap_err(), Error::InvalidMagic);
	}
}
