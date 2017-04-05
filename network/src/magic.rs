//! Bitcoin network magic number
//! https://www.anintegratedworld.com/unravelling-the-mysterious-block-chain-magic-number/

use compact::Compact;
use ser::{Stream, Serializable};
use chain::Block;
use super::ConsensusParams;

const MAGIC_MAINNET: u32 = 0xD9B4BEF9;
const MAGIC_TESTNET: u32 = 0x0709110B;
const MAGIC_REGTEST: u32 = 0xDAB5BFFA;
const MAGIC_UNITEST: u32 = 0x00000000;

const MAX_BITS_MAINNET: u32 = 0x1d00ffff;
const MAX_BITS_TESTNET: u32 = 0x1d00ffff;
const MAX_BITS_REGTEST: u32 = 0x207fffff;

/// Bitcoin [network](https://bitcoin.org/en/glossary/mainnet)
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Magic {
	/// The original and main network for Bitcoin transactions, where satoshis have real economic value.
	Mainnet,
	/// The main bitcoin testnet.
	Testnet,
	/// Bitcoin regtest network.
	Regtest,
	/// Testnet for unittests, proof of work difficulty is almost 0
	Unitest,
	/// Any other network. By default behaves like bitcoin mainnet.
	Other(u32),
}

impl From<Magic> for u32 {
	fn from(m: Magic) -> Self {
		match m {
			Magic::Mainnet => MAGIC_MAINNET,
			Magic::Testnet => MAGIC_TESTNET,
			Magic::Regtest => MAGIC_REGTEST,
			Magic::Unitest => MAGIC_UNITEST,
			Magic::Other(magic) => magic,
		}
	}
}

impl From<u32> for Magic {
	fn from(u: u32) -> Self {
		match u {
			MAGIC_MAINNET => Magic::Mainnet,
			MAGIC_TESTNET => Magic::Testnet,
			MAGIC_REGTEST => Magic::Regtest,
			MAGIC_UNITEST => Magic::Unitest,
			other => Magic::Other(other),
		}
	}
}

impl Magic {
	pub fn max_bits(&self) -> Compact {
		match *self {
			Magic::Mainnet | Magic::Other(_) => MAX_BITS_MAINNET.into(),
			Magic::Testnet => MAX_BITS_TESTNET.into(),
			Magic::Regtest => MAX_BITS_REGTEST.into(),
			Magic::Unitest => Compact::max_value(),
		}
	}

	pub fn port(&self) -> u16 {
		match *self {
			Magic::Mainnet | Magic::Other(_)  => 8333,
			Magic::Testnet => 18333,
			Magic::Regtest | Magic::Unitest => 18444,
		}
	}

	pub fn rpc_port(&self) -> u16 {
		match *self {
			Magic::Mainnet | Magic::Other(_) => 8332,
			Magic::Testnet => 18332,
			Magic::Regtest | Magic::Unitest => 18443,
		}
	}

	pub fn genesis_block(&self) -> Block {
		match *self {
			Magic::Mainnet | Magic::Other(_) => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
			Magic::Testnet => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4adae5494dffff001d1aa4ae180101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
			Magic::Regtest | Magic::Unitest => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4adae5494dffff7f20020000000101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
		}
	}

	pub fn consensus_params(&self) -> ConsensusParams {
		ConsensusParams::with_magic(*self)
	}
}

impl Serializable for Magic {
	fn serialize(&self, stream: &mut Stream) {
		stream.append(&u32::from(*self));
	}
}

#[cfg(test)]
mod tests {
	use compact::Compact;
	use super::{
		Magic, MAGIC_MAINNET, MAGIC_TESTNET, MAGIC_REGTEST, MAGIC_UNITEST,
		MAX_BITS_MAINNET, MAX_BITS_TESTNET, MAX_BITS_REGTEST,
	};

	#[test]
	fn test_network_magic_number() {
		assert_eq!(MAGIC_MAINNET, Magic::Mainnet.into());
		assert_eq!(MAGIC_TESTNET, Magic::Testnet.into());
		assert_eq!(MAGIC_REGTEST, Magic::Regtest.into());
		assert_eq!(MAGIC_UNITEST, Magic::Unitest.into());
		assert_eq!(Magic::Mainnet, MAGIC_MAINNET.into());
		assert_eq!(Magic::Testnet, MAGIC_TESTNET.into());
		assert_eq!(Magic::Regtest, MAGIC_REGTEST.into());
		assert_eq!(Magic::Unitest, MAGIC_UNITEST.into());
		assert_eq!(Magic::Other(1), 1.into());
	}

	#[test]
	fn test_network_max_bits() {
		assert_eq!(Magic::Mainnet.max_bits(), MAX_BITS_MAINNET.into());
		assert_eq!(Magic::Testnet.max_bits(), MAX_BITS_TESTNET.into());
		assert_eq!(Magic::Regtest.max_bits(), MAX_BITS_REGTEST.into());
		assert_eq!(Magic::Unitest.max_bits(), Compact::max_value());
	}

	#[test]
	fn test_network_port() {
		assert_eq!(Magic::Mainnet.port(), 8333);
		assert_eq!(Magic::Testnet.port(), 18333);
		assert_eq!(Magic::Regtest.port(), 18444);
		assert_eq!(Magic::Unitest.port(), 18444);
	}

	#[test]
	fn test_network_rpc_port() {
		assert_eq!(Magic::Mainnet.rpc_port(), 8332);
		assert_eq!(Magic::Testnet.rpc_port(), 18332);
		assert_eq!(Magic::Regtest.rpc_port(), 18443);
		assert_eq!(Magic::Unitest.rpc_port(), 18443);
	}
}
