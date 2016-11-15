use super::Magic;

#[derive(Debug, Clone)]
/// Parameters that influence chain consensus.
pub struct ConsensusParams {
	/// Block height at which BIP65 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki
	pub bip65_height: u32,
}

impl ConsensusParams {
	pub fn with_magic(magic: Magic) -> Self {
		match magic {
			Magic::Mainnet => ConsensusParams {
				bip65_height: 388381, // 000000000000000004c2b624ed5d7756c508d90fd0da2c7c679febfa6c4735f0
			},
			Magic::Testnet => ConsensusParams {
				bip65_height: 581885, // 00000000007f6655f22f98e72ed80d8b06dc761d5da09df0fa1dc4be4f861eb6
			},
			Magic::Regtest => ConsensusParams {
				bip65_height: 1351,
			},
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::Magic;
	use super::ConsensusParams;

	#[test]
	fn test_consensus_params_bip65_height() {
		assert_eq!(ConsensusParams::with_magic(Magic::Mainnet).bip65_height, 388381);
		assert_eq!(ConsensusParams::with_magic(Magic::Testnet).bip65_height, 581885);
		assert_eq!(ConsensusParams::with_magic(Magic::Regtest).bip65_height, 1351);
	}
}
