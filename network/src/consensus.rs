use std::cmp::max;
use hash::H256;
use {Magic, Deployment};

/// First block of SegWit2x fork.
pub const SEGWIT2X_FORK_BLOCK: u32 = 0xFFFFFFFF; // not known (yet?)
/// First block of BitcoinCash fork.
pub const BITCOIN_CASH_FORK_BLOCK: u32 = 478559; // https://blockchair.com/bitcoin-cash/block/478559

/// Segwit-related constants.
pub mod segwit {
	/// The maximum allowed weight for a block, see BIP 141 (network rule).
	pub const MAX_BLOCK_WEIGHT: usize = 4_000_000;
	/// The maximum allowed number of signature check operations in a block (network rule).
	pub const MAX_BLOCK_SIGOPS_COST: usize = 80_000;
	/// Witness scale factor.
	pub const WITNESS_SCALE_FACTOR: usize = 4;
}

#[derive(Debug, Clone)]
/// Parameters that influence chain consensus.
pub struct ConsensusParams {
	/// Network.
	pub network: Magic,
	/// Time when BIP16 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0016.mediawiki
	pub bip16_time: u32,
	/// Block height at which BIP34 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0034.mediawiki
	pub bip34_height: u32,
	/// Block height at which BIP65 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki
	pub bip65_height: u32,
	/// Block height at which BIP65 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0066.mediawiki
	pub bip66_height: u32,
	/// Selected consensus fork.
	pub fork: ConsensusFork,
	/// Version bits activation
	pub rule_change_activation_threshold: u32,
	/// Number of blocks with the same set of rules
	pub miner_confirmation_window: u32,
	/// BIP68, BIP112, BIP113 deployment
	pub csv_deployment: Option<Deployment>,
	/// BIP141, BIP143, BIP147 deployment
	pub segwit_deployment: Option<Deployment>,
}

#[derive(Debug, Clone, Copy)]
/// Concurrent consensus rule forks.
pub enum ConsensusFork {
	/// No fork.
	NoFork,
	/// SegWit2x (aka The New York Agreement).
	/// `u32` is height of the first block, for which new consensus rules are applied.
	/// Briefly: SegWit + blocks up to 2MB.
	/// Technical specification:
	/// Segregated Witness (Consensus layer) - https://github.com/bitcoin/bips/blob/master/bip-0141.mediawiki
	/// Block size increase to 2MB - https://github.com/bitcoin/bips/blob/master/bip-0102.mediawiki
	SegWit2x(u32),
	/// Bitcoin Cash (aka UAHF).
	/// `u32` is height of the first block, for which new consensus rules are applied.
	/// Briefly: no SegWit + blocks up to 8MB + replay protection.
	/// Technical specification:
	/// UAHF Technical Specification - https://github.com/Bitcoin-UAHF/spec/blob/master/uahf-technical-spec.md
	/// BUIP-HF Digest for replay protected signature verification across hard forks - https://github.com/Bitcoin-UAHF/spec/blob/master/replay-protected-sighash.md
	BitcoinCash(u32),
}

impl ConsensusParams {
	pub fn new(magic: Magic, fork: ConsensusFork) -> Self {
		match magic {
			Magic::Mainnet | Magic::Other(_) => ConsensusParams {
				network: magic,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 227931,	// 000000000000024b89b42a942fe0d9fea3bb44ab7bd1b19115dd6a759c0808b8
				bip65_height: 388381,	// 000000000000000004c2b624ed5d7756c508d90fd0da2c7c679febfa6c4735f0
				bip66_height: 363725,	// 00000000000000000379eaa19dce8c9b722d46ae6a57c2f1a988119488b50931
				fork: fork,
				rule_change_activation_threshold: 1916, // 95%
				miner_confirmation_window: 2016,
				csv_deployment: Some(Deployment {
					name: "csv",
					bit: 0,
					start_time: 1462060800,
					timeout: 1493596800,
					activation: Some(770112),
				}),
				segwit_deployment: match fork {
					ConsensusFork::NoFork | ConsensusFork::SegWit2x(_) => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 1479168000,
						timeout: 1510704000,
						activation: None,
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
			},
			Magic::Testnet => ConsensusParams {
				network: magic,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 21111,	// 0000000023b3a96d3484e5abb3755c413e7d41500f8e2a5c3f0dd01299cd8ef8
				bip65_height: 581885,	// 00000000007f6655f22f98e72ed80d8b06dc761d5da09df0fa1dc4be4f861eb6
				bip66_height: 330776,	// 000000002104c8c45e99a8853285a3b592602a3ccde2b832481da85e9e4ba182
				fork: fork,
				rule_change_activation_threshold: 1512, // 75%
				miner_confirmation_window: 2016,
				csv_deployment: Some(Deployment {
					name: "csv",
					bit: 0,
					start_time: 1456790400,
					timeout: 1493596800,
					activation: Some(419328),
				}),
				segwit_deployment: match fork {
					ConsensusFork::NoFork | ConsensusFork::SegWit2x(_) => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 1462060800,
						timeout: 1493596800,
						activation: None,
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
			},
			Magic::Regtest | Magic::Unitest => ConsensusParams {
				network: magic,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 100000000,	// not activated on regtest
				bip65_height: 1351,
				bip66_height: 1251,		// used only in rpc tests
				fork: fork,
				rule_change_activation_threshold: 108, // 75%
				miner_confirmation_window: 144,
				csv_deployment: Some(Deployment {
					name: "csv",
					bit: 0,
					start_time: 0,
					timeout: 0,
					activation: Some(0),
				}),
				segwit_deployment: match fork {
					ConsensusFork::NoFork | ConsensusFork::SegWit2x(_) => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 0,
						timeout: ::std::u32::MAX,
						activation: None,
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
			},
		}
	}

	pub fn is_bip30_exception(&self, hash: &H256, height: u32) -> bool {
		(height == 91842 && hash == &H256::from_reversed_str("00000000000a4d0a398161ffc163c503763b1f4360639393e0e4c8e300e0caec")) ||
		(height == 91880 && hash == &H256::from_reversed_str("00000000000743f190a18c5577a3c2d2a1f610ae9601ac046a38084ccb7cd721"))
	}
}

impl ConsensusFork {
	/// Absolute (across all forks) maximum block size. Currently is 8MB for post-HF BitcoinCash
	pub fn absolute_maximum_block_size() -> usize {
		8_000_000
	}

	/// Absolute (across all forks) maximum number of sigops in single block. Currently is max(sigops) for 8MB post-HF BitcoinCash block
	pub fn absolute_maximum_block_sigops() -> usize {
		160_000
	}

	pub fn max_transaction_size(&self) -> usize {
		// BitcoinCash: according to REQ-5: max size of tx is still 1_000_000
		// SegWit: size * 4 <= 4_000_000 ===> max size of tx is still 1_000_000
 		1_000_000
	}

	pub fn min_block_size(&self, height: u32) -> usize {
		match *self {
			// size of first fork block must be larger than 1MB
			ConsensusFork::BitcoinCash(fork_height) if height == fork_height => 1_000_001,
			ConsensusFork::NoFork | ConsensusFork::BitcoinCash(_) | ConsensusFork::SegWit2x(_) => 0,
		}
	}

	pub fn max_block_size(&self, height: u32) -> usize {
		match *self {
			ConsensusFork::SegWit2x(fork_height) if height >= fork_height => 2_000_000,
			ConsensusFork::BitcoinCash(fork_height) if height >= fork_height => 8_000_000,
			ConsensusFork::NoFork | ConsensusFork::BitcoinCash(_) | ConsensusFork::SegWit2x(_) => 1_000_000,
		}
	}

	pub fn max_block_sigops(&self, height: u32, block_size: usize) -> usize {
		match *self {
			// according to REQ-5: max_block_sigops = 20000 * ceil((max(blocksize_bytes, 1000000) / 1000000))
			ConsensusFork::BitcoinCash(fork_height) if height >= fork_height && block_size > 1_000_000 =>
				20_000 * (max(block_size, 1_000_000) / 1_000_000),
			ConsensusFork::NoFork | ConsensusFork::SegWit2x(_) | ConsensusFork::BitcoinCash(_) => 20_000,
		}
	}

	pub fn max_block_sigops_cost(&self, height: u32, block_size: usize) -> usize {
		match *self {
			ConsensusFork::BitcoinCash(_) =>
				self.max_block_sigops(height, block_size) * segwit::WITNESS_SCALE_FACTOR,
			ConsensusFork::NoFork | ConsensusFork::SegWit2x(_) =>
				segwit::MAX_BLOCK_SIGOPS_COST,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::Magic;
	use super::{ConsensusParams, ConsensusFork};
	//use deployments::tests::DummyDeployments;

	#[test]
	fn test_consensus_params_bip34_height() {
		assert_eq!(ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork).bip34_height, 227931);
		assert_eq!(ConsensusParams::new(Magic::Testnet, ConsensusFork::NoFork).bip34_height, 21111);
		assert_eq!(ConsensusParams::new(Magic::Regtest, ConsensusFork::NoFork).bip34_height, 100000000);
	}

	#[test]
	fn test_consensus_params_bip65_height() {
		assert_eq!(ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork).bip65_height, 388381);
		assert_eq!(ConsensusParams::new(Magic::Testnet, ConsensusFork::NoFork).bip65_height, 581885);
		assert_eq!(ConsensusParams::new(Magic::Regtest, ConsensusFork::NoFork).bip65_height, 1351);
	}

	#[test]
	fn test_consensus_params_bip66_height() {
		assert_eq!(ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork).bip66_height, 363725);
		assert_eq!(ConsensusParams::new(Magic::Testnet, ConsensusFork::NoFork).bip66_height, 330776);
		assert_eq!(ConsensusParams::new(Magic::Regtest, ConsensusFork::NoFork).bip66_height, 1251);
	}

	#[test]
	fn test_consensus_activation_threshold() {
		assert_eq!(ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork).rule_change_activation_threshold, 1916);
		assert_eq!(ConsensusParams::new(Magic::Testnet, ConsensusFork::NoFork).rule_change_activation_threshold, 1512);
		assert_eq!(ConsensusParams::new(Magic::Regtest, ConsensusFork::NoFork).rule_change_activation_threshold, 108);
	}

	#[test]
	fn test_consensus_miner_confirmation_window() {
		assert_eq!(ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork).miner_confirmation_window, 2016);
		assert_eq!(ConsensusParams::new(Magic::Testnet, ConsensusFork::NoFork).miner_confirmation_window, 2016);
		assert_eq!(ConsensusParams::new(Magic::Regtest, ConsensusFork::NoFork).miner_confirmation_window, 144);
	}

	#[test]
	fn test_consensus_fork_max_block_sigops() {
		assert_eq!(ConsensusFork::NoFork.max_block_sigops(0, 1_000_000), 20_000);
		assert_eq!(ConsensusFork::SegWit2x(100).max_block_sigops(0, 1_000_000), 20_000);
		assert_eq!(ConsensusFork::SegWit2x(100).max_block_sigops(100, 2_000_000), 20_000);
		assert_eq!(ConsensusFork::SegWit2x(100).max_block_sigops(200, 3_000_000), 20_000);
		assert_eq!(ConsensusFork::BitcoinCash(100).max_block_sigops(0, 1_000_000), 20_000);
		assert_eq!(ConsensusFork::BitcoinCash(100).max_block_sigops(100, 2_000_000), 40_000);
		assert_eq!(ConsensusFork::BitcoinCash(100).max_block_sigops(200, 3_000_000), 60_000);
	}
}
