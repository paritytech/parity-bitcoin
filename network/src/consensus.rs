use hash::H256;
use {Network, Magic, Deployment};

#[derive(Debug, Clone)]
/// Parameters that influence chain consensus.
pub struct ConsensusParams {
	/// Network.
	pub network: Network,
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

#[derive(Debug, Clone)]
/// Bitcoin cash consensus parameters.
pub struct BitcoinCashConsensusParams {
	/// Initial BCH hard fork height.
	pub height: u32,
	/// Height of difficulty adjustment hardfork.
	/// https://reviews.bitcoinabc.org/D601
	pub difficulty_adjustion_height: u32,
	/// Time of monolith (aka May 2018) hardfork.
	/// https://github.com/bitcoincashorg/spec/blob/4fbb0face661e293bcfafe1a2a4744dcca62e50d/may-2018-hardfork.md
	pub monolith_time: u32,
	/// Time of magnetic anomaly (aka Nov 2018) hardfork.
	/// https://github.com/bitcoincashorg/bitcoincash.org/blob/f92f5412f2ed60273c229f68dd8703b6d5d09617/spec/2018-nov-upgrade.md
	pub magnetic_anomaly_time: u32,
}

#[derive(Debug, Clone)]
/// Concurrent consensus rule forks.
pub enum ConsensusFork {
	/// No fork.
	BitcoinCore,
	/// Bitcoin Cash (aka UAHF).
	/// `u32` is height of the first block, for which new consensus rules are applied.
	/// Briefly: no SegWit + blocks up to 8MB + replay protection.
	/// Technical specification:
	/// UAHF Technical Specification - https://github.com/Bitcoin-UAHF/spec/blob/master/uahf-technical-spec.md
	/// BUIP-HF Digest for replay protected signature verification across hard forks - https://github.com/Bitcoin-UAHF/spec/blob/master/replay-protected-sighash.md
	BitcoinCash(BitcoinCashConsensusParams),
}

#[derive(Debug, Clone, Copy)]
/// Describes the ordering of transactions within single block.
pub enum TransactionOrdering {
	/// Topological tranasaction ordering: if tx TX2 depends on tx TX1,
	/// it should come AFTER TX1 (not necessary **right** after it).
	Topological,
	/// Canonical transaction ordering: transactions are ordered by their
	/// hash (in ascending order).
	Canonical,
}

impl ConsensusParams {
	pub fn new(network: Network, fork: ConsensusFork) -> Self {
		match network {
			Network::Mainnet | Network::Other(_) => ConsensusParams {
				network: network,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 227931,	// 000000000000024b89b42a942fe0d9fea3bb44ab7bd1b19115dd6a759c0808b8
				bip65_height: 388381,	// 000000000000000004c2b624ed5d7756c508d90fd0da2c7c679febfa6c4735f0
				bip66_height: 363725,	// 00000000000000000379eaa19dce8c9b722d46ae6a57c2f1a988119488b50931
				segwit_deployment: match fork {
					ConsensusFork::BitcoinCore => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 1479168000,
						timeout: 1510704000,
						activation: Some(481824),
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
				fork: fork,
				rule_change_activation_threshold: 1916, // 95%
				miner_confirmation_window: 2016,
				csv_deployment: Some(Deployment {
					name: "csv",
					bit: 0,
					start_time: 1462060800,
					timeout: 1493596800,
					activation: Some(419328),
				}),
			},
			Network::Testnet => ConsensusParams {
				network: network,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 21111,	// 0000000023b3a96d3484e5abb3755c413e7d41500f8e2a5c3f0dd01299cd8ef8
				bip65_height: 581885,	// 00000000007f6655f22f98e72ed80d8b06dc761d5da09df0fa1dc4be4f861eb6
				bip66_height: 330776,	// 000000002104c8c45e99a8853285a3b592602a3ccde2b832481da85e9e4ba182
				segwit_deployment: match fork {
					ConsensusFork::BitcoinCore => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 1462060800,
						timeout: 1493596800,
						activation: Some(834624),
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
				fork: fork,
				rule_change_activation_threshold: 1512, // 75%
				miner_confirmation_window: 2016,
				csv_deployment: Some(Deployment {
					name: "csv",
					bit: 0,
					start_time: 1456790400,
					timeout: 1493596800,
					activation: Some(770112),
				}),
			},
			Network::Regtest | Network::Unitest => ConsensusParams {
				network: network,
				bip16_time: 1333238400,	// Apr 1 2012
				bip34_height: 100000000,	// not activated on regtest
				bip65_height: 1351,
				bip66_height: 1251,		// used only in rpc tests
				segwit_deployment: match fork {
					ConsensusFork::BitcoinCore => Some(Deployment {
						name: "segwit",
						bit: 1,
						start_time: 0,
						timeout: ::std::u32::MAX,
						activation: None,
					}),
					ConsensusFork::BitcoinCash(_) => None,
				},
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
			},
		}
	}

	pub fn magic(&self) -> Magic {
		self.network.magic(&self.fork)
	}

	pub fn is_bip30_exception(&self, hash: &H256, height: u32) -> bool {
		(height == 91842 && hash == &H256::from_reversed_str("00000000000a4d0a398161ffc163c503763b1f4360639393e0e4c8e300e0caec")) ||
		(height == 91880 && hash == &H256::from_reversed_str("00000000000743f190a18c5577a3c2d2a1f610ae9601ac046a38084ccb7cd721"))
	}

	/// Returns true if SegWit is possible on this chain.
	pub fn is_segwit_possible(&self) -> bool {
		match self.fork {
			// SegWit is not supported in (our?) regtests
			ConsensusFork::BitcoinCore if self.network != Network::Regtest => true,
			ConsensusFork::BitcoinCore | ConsensusFork::BitcoinCash(_) => false,
		}
	}
}

impl ConsensusFork {
	/// Absolute (across all forks) maximum block size. Currently is 8MB for post-HF BitcoinCash
	pub fn absolute_maximum_block_size() -> usize {
		32_000_000
	}

	/// Absolute (across all forks) maximum number of sigops in single block. Currently is max(sigops) for 8MB post-HF BitcoinCash block
	pub fn absolute_maximum_block_sigops() -> usize {
		160_000
	}

	/// Witness scale factor (equal among all forks)
	pub fn witness_scale_factor() -> usize {
		4
	}

	pub fn activation_height(&self) -> u32 {
		match *self {
			ConsensusFork::BitcoinCore => 0,
			ConsensusFork::BitcoinCash(ref fork) => fork.height,
		}
	}

	pub fn min_transaction_size(&self, median_time_past: u32) -> usize {
		match *self {
			ConsensusFork::BitcoinCash(ref fork) if median_time_past >= fork.magnetic_anomaly_time => 100,
			_ => 0,
		}
	}

	pub fn max_transaction_size(&self) -> usize {
		// BitcoinCash: according to REQ-5: max size of tx is still 1_000_000
		// SegWit: size * 4 <= 4_000_000 ===> max size of tx is still 1_000_000
 		1_000_000
	}

	pub fn min_block_size(&self, height: u32) -> usize {
		match *self {
			// size of first fork block must be larger than 1MB
			ConsensusFork::BitcoinCash(ref fork) if height == fork.height => 1_000_001,
			ConsensusFork::BitcoinCore | ConsensusFork::BitcoinCash(_) => 0,
		}
	}

	pub fn max_block_size(&self, height: u32, median_time_past: u32) -> usize {
		match *self {
			ConsensusFork::BitcoinCash(ref fork) if median_time_past >= fork.monolith_time => 32_000_000,
			ConsensusFork::BitcoinCash(ref fork) if height >= fork.height => 8_000_000,
			ConsensusFork::BitcoinCore | ConsensusFork::BitcoinCash(_) => 1_000_000,
		}
	}

	pub fn max_block_sigops(&self, height: u32, block_size: usize) -> usize {
		match *self {
			// according to REQ-5: max_block_sigops = 20000 * ceil((max(blocksize_bytes, 1000000) / 1000000))
			ConsensusFork::BitcoinCash(ref fork) if height >= fork.height =>
				20_000 * (1 + (block_size - 1) / 1_000_000),
			ConsensusFork::BitcoinCore | ConsensusFork::BitcoinCash(_) => 20_000,
		}
	}

	pub fn max_block_sigops_cost(&self, height: u32, block_size: usize) -> usize {
		match *self {
			ConsensusFork::BitcoinCash(_) =>
				self.max_block_sigops(height, block_size) * Self::witness_scale_factor(),
			ConsensusFork::BitcoinCore =>
				80_000,
		}
	}

	pub fn max_block_weight(&self, _height: u32) -> usize {
		match *self {
			ConsensusFork::BitcoinCore =>
				4_000_000,
			ConsensusFork::BitcoinCash(_) =>
				unreachable!("BitcoinCash has no SegWit; weight is only checked with SegWit activated; qed"),
		}
	}

	pub fn transaction_ordering(&self, median_time_past: u32) -> TransactionOrdering {
		match *self {
			ConsensusFork::BitcoinCash(ref fork) if median_time_past >= fork.magnetic_anomaly_time
				=> TransactionOrdering::Canonical,
			_ => TransactionOrdering::Topological,
		}
	}
}

impl BitcoinCashConsensusParams {
	pub fn new(network: Network) -> Self {
		match network {
			Network::Mainnet | Network::Other(_) => BitcoinCashConsensusParams {
				height: 478559,
				difficulty_adjustion_height: 504031,
				monolith_time: 1526400000,
				magnetic_anomaly_time: 1542300000,
			},
			Network::Testnet => BitcoinCashConsensusParams {
				height: 1155876,
				difficulty_adjustion_height: 1188697,
				monolith_time: 1526400000,
				magnetic_anomaly_time: 1542300000,
			},
			Network::Regtest | Network::Unitest => BitcoinCashConsensusParams {
				height: 0,
				difficulty_adjustion_height: 0,
				monolith_time: 1526400000,
				magnetic_anomaly_time: 1542300000,
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::Network;
	use super::{ConsensusParams, ConsensusFork, BitcoinCashConsensusParams};

	#[test]
	fn test_consensus_params_bip34_height() {
		assert_eq!(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore).bip34_height, 227931);
		assert_eq!(ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore).bip34_height, 21111);
		assert_eq!(ConsensusParams::new(Network::Regtest, ConsensusFork::BitcoinCore).bip34_height, 100000000);
	}

	#[test]
	fn test_consensus_params_bip65_height() {
		assert_eq!(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore).bip65_height, 388381);
		assert_eq!(ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore).bip65_height, 581885);
		assert_eq!(ConsensusParams::new(Network::Regtest, ConsensusFork::BitcoinCore).bip65_height, 1351);
	}

	#[test]
	fn test_consensus_params_bip66_height() {
		assert_eq!(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore).bip66_height, 363725);
		assert_eq!(ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore).bip66_height, 330776);
		assert_eq!(ConsensusParams::new(Network::Regtest, ConsensusFork::BitcoinCore).bip66_height, 1251);
	}

	#[test]
	fn test_consensus_activation_threshold() {
		assert_eq!(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore).rule_change_activation_threshold, 1916);
		assert_eq!(ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore).rule_change_activation_threshold, 1512);
		assert_eq!(ConsensusParams::new(Network::Regtest, ConsensusFork::BitcoinCore).rule_change_activation_threshold, 108);
	}

	#[test]
	fn test_consensus_miner_confirmation_window() {
		assert_eq!(ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCore).miner_confirmation_window, 2016);
		assert_eq!(ConsensusParams::new(Network::Testnet, ConsensusFork::BitcoinCore).miner_confirmation_window, 2016);
		assert_eq!(ConsensusParams::new(Network::Regtest, ConsensusFork::BitcoinCore).miner_confirmation_window, 144);
	}

	#[test]
	fn test_consensus_fork_min_block_size() {
		assert_eq!(ConsensusFork::BitcoinCore.min_block_size(0), 0);
		let fork = ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet));
		assert_eq!(fork.min_block_size(0), 0);
		assert_eq!(fork.min_block_size(fork.activation_height()), 1_000_001);
	}

	#[test]
	fn test_consensus_fork_max_transaction_size() {
		assert_eq!(ConsensusFork::BitcoinCore.max_transaction_size(), 1_000_000);
		assert_eq!(ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet)).max_transaction_size(), 1_000_000);
	}

	#[test]
	fn test_consensus_fork_min_transaction_size() {
		assert_eq!(ConsensusFork::BitcoinCore.min_transaction_size(0), 0);
		assert_eq!(ConsensusFork::BitcoinCore.min_transaction_size(2000000000), 0);
		assert_eq!(ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet)).min_transaction_size(0), 0);
		assert_eq!(ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet)).min_transaction_size(2000000000), 100);
	}

	#[test]
	fn test_consensus_fork_max_block_sigops() {
		assert_eq!(ConsensusFork::BitcoinCore.max_block_sigops(0, 1_000_000), 20_000);
		let fork = ConsensusFork::BitcoinCash(BitcoinCashConsensusParams::new(Network::Mainnet));
		assert_eq!(fork.max_block_sigops(0, 1_000_000), 20_000);
		assert_eq!(fork.max_block_sigops(fork.activation_height(), 2_000_000), 40_000);
		assert_eq!(fork.max_block_sigops(fork.activation_height() + 100, 3_000_000), 60_000);
	}
}
