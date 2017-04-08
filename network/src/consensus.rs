use hash::H256;
use super::Magic;

#[derive(Debug, Clone)]
/// Parameters that influence chain consensus.
pub struct ConsensusParams {
	/// Time when BIP16 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0016.mediawiki
	pub bip16_time: u32,
	/// Block height at which BIP65 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki
	pub bip65_height: u32,
	/// Block height at which BIP65 becomes active.
	/// See https://github.com/bitcoin/bips/blob/master/bip-0065.mediawiki
	pub bip66_height: u32,
}

impl ConsensusParams {
	pub fn with_magic(magic: Magic) -> Self {
		match magic {
			Magic::Mainnet | Magic::Other(_) => ConsensusParams {
				bip16_time: 1333238400,	// Apr 1 2012
				bip65_height: 388381,	// 000000000000000004c2b624ed5d7756c508d90fd0da2c7c679febfa6c4735f0
				bip66_height: 363725,	// 00000000000000000379eaa19dce8c9b722d46ae6a57c2f1a988119488b50931
			},
			Magic::Testnet => ConsensusParams {
				bip16_time: 1333238400,	// Apr 1 2012
				bip65_height: 581885,	// 00000000007f6655f22f98e72ed80d8b06dc761d5da09df0fa1dc4be4f861eb6
				bip66_height: 330776,	// 000000002104c8c45e99a8853285a3b592602a3ccde2b832481da85e9e4ba182
			},
			Magic::Regtest | Magic::Unitest => ConsensusParams {
				bip16_time: 1333238400,	// Apr 1 2012
				bip65_height: 1351,
				bip66_height: 1251,
			},
		}
	}

	pub fn is_bip30_exception(&self, hash: &H256, height: u32) -> bool {
		(height == 91842 && hash == &H256::from_reversed_str("00000000000a4d0a398161ffc163c503763b1f4360639393e0e4c8e300e0caec")) ||
		(height == 91880 && hash == &H256::from_reversed_str("00000000000743f190a18c5577a3c2d2a1f610ae9601ac046a38084ccb7cd721"))
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

	#[test]
	fn test_consensus_params_bip66_height() {
		assert_eq!(ConsensusParams::with_magic(Magic::Mainnet).bip66_height, 363725);
		assert_eq!(ConsensusParams::with_magic(Magic::Testnet).bip66_height, 330776);
		assert_eq!(ConsensusParams::with_magic(Magic::Regtest).bip66_height, 1251);
	}
}
