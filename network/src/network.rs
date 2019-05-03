//! Bitcoin network
//! https://www.anintegratedworld.com/unravelling-the-mysterious-block-chain-magic-number/

use compact::Compact;
use chain::IndexedBlock;
use primitives::hash::H256;
use primitives::bigint::U256;
use {ConsensusFork};

const MAGIC_MAINNET: u32 = 0xD9B4BEF9;
const MAGIC_TESTNET: u32 = 0x0709110B;
const MAGIC_REGTEST: u32 = 0xDAB5BFFA;
const MAGIC_UNITEST: u32 = 0x00000000;

const BITCOIN_CASH_MAGIC_MAINNET: u32 = 0xE8F3E1E3;
const BITCOIN_CASH_MAGIC_TESTNET: u32 = 0xF4F3E5F4;
const BITCOIN_CASH_MAGIC_REGTEST: u32 = 0xFABFB5DA;

lazy_static! {
	static ref MAX_BITS_MAINNET: U256 = "00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff".parse()
		.expect("hardcoded value should parse without errors");
	static ref MAX_BITS_TESTNET: U256 = "00000000ffffffffffffffffffffffffffffffffffffffffffffffffffffffff".parse()
		.expect("hardcoded value should parse without errors");
	static ref MAX_BITS_REGTEST: U256 = "7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".parse()
		.expect("hardcoded value should parse without errors");
}

/// Network magic type.
pub type Magic = u32;

/// Bitcoin [network](https://bitcoin.org/en/glossary/mainnet)
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Network {
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

impl Network {
	pub fn magic(&self, fork: &ConsensusFork) -> Magic {
		match (fork, *self) {
			(&ConsensusFork::BitcoinCash(_), Network::Mainnet) => BITCOIN_CASH_MAGIC_MAINNET,
			(&ConsensusFork::BitcoinCash(_), Network::Testnet) => BITCOIN_CASH_MAGIC_TESTNET,
			(&ConsensusFork::BitcoinCash(_), Network::Regtest) => BITCOIN_CASH_MAGIC_REGTEST,
			(_, Network::Mainnet) => MAGIC_MAINNET,
			(_, Network::Testnet) => MAGIC_TESTNET,
			(_, Network::Regtest) => MAGIC_REGTEST,
			(_, Network::Unitest) => MAGIC_UNITEST,
			(_, Network::Other(value)) => value,
		}
	}

	pub fn max_bits(&self) -> U256 {
		match *self {
			Network::Mainnet | Network::Other(_) => MAX_BITS_MAINNET.clone(),
			Network::Testnet => MAX_BITS_TESTNET.clone(),
			Network::Regtest => MAX_BITS_REGTEST.clone(),
			Network::Unitest => Compact::max_value().into(),
		}
	}

	pub fn port(&self) -> u16 {
		match *self {
			Network::Mainnet | Network::Other(_) => 8333,
			Network::Testnet => 18333,
			Network::Regtest | Network::Unitest => 18444,
		}
	}

	pub fn rpc_port(&self) -> u16 {
		match *self {
			Network::Mainnet | Network::Other(_) => 8332,
			Network::Testnet => 18332,
			Network::Regtest | Network::Unitest => 18443,
		}
	}

	pub fn genesis_block(&self) -> IndexedBlock {
		match *self {
			Network::Mainnet | Network::Other(_) => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4a29ab5f49ffff001d1dac2b7c0101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
			Network::Testnet => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4adae5494dffff001d1aa4ae180101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
			Network::Regtest | Network::Unitest => "0100000000000000000000000000000000000000000000000000000000000000000000003ba3edfd7a7b12b27ac72c3e67768f617fc81bc3888a51323a9fb8aa4b1e5e4adae5494dffff7f20020000000101000000010000000000000000000000000000000000000000000000000000000000000000ffffffff4d04ffff001d0104455468652054696d65732030332f4a616e2f32303039204368616e63656c6c6f72206f6e206272696e6b206f66207365636f6e64206261696c6f757420666f722062616e6b73ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000".into(),
		}
	}

	pub fn default_verification_edge(&self) -> H256 {
		match *self {
			Network::Mainnet => H256::from_reversed_str("0000000000000000030abc968e1bd635736e880b946085c93152969b9a81a6e2"),
			Network::Testnet => H256::from_reversed_str("000000000871ee6842d3648317ccc8a435eb8cc3c2429aee94faff9ba26b05a0"),
			_ => *self.genesis_block().hash(),
		}
	}
}

#[cfg(test)]
mod tests {
	use compact::Compact;
	use {ConsensusFork};
	use super::{
		Network, MAGIC_MAINNET, MAGIC_TESTNET, MAGIC_REGTEST, MAGIC_UNITEST,
		MAX_BITS_MAINNET, MAX_BITS_TESTNET, MAX_BITS_REGTEST,
	};

	#[test]
	fn test_network_magic_number() {
		assert_eq!(MAGIC_MAINNET, Network::Mainnet.magic(&ConsensusFork::BitcoinCore));
		assert_eq!(MAGIC_TESTNET, Network::Testnet.magic(&ConsensusFork::BitcoinCore));
		assert_eq!(MAGIC_REGTEST, Network::Regtest.magic(&ConsensusFork::BitcoinCore));
		assert_eq!(MAGIC_UNITEST, Network::Unitest.magic(&ConsensusFork::BitcoinCore));
	}

	#[test]
	fn test_network_max_bits() {
		assert_eq!(Network::Mainnet.max_bits(), *MAX_BITS_MAINNET);
		assert_eq!(Network::Testnet.max_bits(), *MAX_BITS_TESTNET);
		assert_eq!(Network::Regtest.max_bits(), *MAX_BITS_REGTEST);
		assert_eq!(Network::Unitest.max_bits(), Compact::max_value().into());
	}

	#[test]
	fn test_network_port() {
		assert_eq!(Network::Mainnet.port(), 8333);
		assert_eq!(Network::Testnet.port(), 18333);
		assert_eq!(Network::Regtest.port(), 18444);
		assert_eq!(Network::Unitest.port(), 18444);
	}

	#[test]
	fn test_network_rpc_port() {
		assert_eq!(Network::Mainnet.rpc_port(), 8332);
		assert_eq!(Network::Testnet.rpc_port(), 18332);
		assert_eq!(Network::Regtest.rpc_port(), 18443);
		assert_eq!(Network::Unitest.rpc_port(), 18443);
	}
}
