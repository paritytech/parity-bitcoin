use std::cmp;
use primitives::compact::Compact;
use primitives::hash::H256;
use primitives::bigint::U256;
use network::{Magic, ConsensusParams, ConsensusFork};
use db::{BlockHeaderProvider, BlockRef};
use timestamp::median_timestamp_inclusive;

use constants::{
	DOUBLE_SPACING_SECONDS,
	TARGET_TIMESPAN_SECONDS, MIN_TIMESPAN, MAX_TIMESPAN, RETARGETING_INTERVAL
};

pub fn is_retarget_height(height: u32) -> bool {
	height % RETARGETING_INTERVAL == 0
}

fn range_constrain(value: i64, min: i64, max: i64) -> i64 {
	cmp::min(cmp::max(value, min), max)
}

/// Returns true if hash is lower or equal than target represented by compact bits
pub fn is_valid_proof_of_work_hash(bits: Compact, hash: &H256) -> bool {
	let target = match bits.to_u256() {
		Ok(target) => target,
		_err => return false,
	};

	let value = U256::from(&*hash.reversed() as &[u8]);
	value <= target
}

/// Returns true if hash is lower or equal than target and target is lower or equal
/// than current network maximum
pub fn is_valid_proof_of_work(max_work_bits: Compact, bits: Compact, hash: &H256) -> bool {
	let maximum = match max_work_bits.to_u256() {
		Ok(max) => max,
		_err => return false,
	};

	let target = match bits.to_u256() {
		Ok(target) => target,
		_err => return false,
	};

	let value = U256::from(&*hash.reversed() as &[u8]);
	target <= maximum && value <= target
}

/// Returns constrained number of seconds since last retarget
pub fn retarget_timespan(retarget_timestamp: u32, last_timestamp: u32) -> u32 {
	// subtract unsigned 32 bit numbers in signed 64 bit space in
	// order to prevent underflow before applying the range constraint.
	let timespan = last_timestamp as i64 - retarget_timestamp as i64;
	range_constrain(timespan, MIN_TIMESPAN as i64, MAX_TIMESPAN as i64) as u32
}

/// Returns work required for given header
pub fn work_required(parent_hash: H256, time: u32, height: u32, store: &BlockHeaderProvider, consensus: &ConsensusParams) -> Compact {
	let max_bits = consensus.network.max_bits();
	if height == 0 {
		return max_bits;
	}

	let parent_header = store.block_header(parent_hash.clone().into()).expect("self.height != 0; qed");

	if is_retarget_height(height) {
		let retarget_ref = (height - RETARGETING_INTERVAL).into();
		let retarget_header = store.block_header(retarget_ref).expect("self.height != 0 && self.height % RETARGETING_INTERVAL == 0; qed");

		// timestamp of block(height - RETARGETING_INTERVAL)
		let retarget_timestamp = retarget_header.time;
		// timestamp of parent block
		let last_timestamp = parent_header.time;
		// bits of last block
		let last_bits = parent_header.bits;

		return work_required_retarget(max_bits, retarget_timestamp, last_timestamp, last_bits);
	}

	if consensus.network == Magic::Testnet {
		return work_required_testnet(parent_hash, time, height, store, Magic::Testnet)
	}

	match consensus.fork {
		_ if parent_header.bits == max_bits => parent_header.bits,
		ConsensusFork::BitcoinCash(fork_height) if height >= fork_height => {
			// REQ-7 Difficulty adjustement in case of hashrate drop
			// In case the MTP of the tip of the chain is 12h or more after the MTP 6 block before the tip,
			// the proof of work target is increased by a quarter, or 25%, which corresponds to a difficulty
			// reduction of 20%.
			let ancient_block_ref = (height - 6 - 1).into();
			let ancient_header = store.block_header(ancient_block_ref)
				.expect("parent_header.bits != max_bits; difficulty is max_bits for first RETARGETING_INTERVAL height; RETARGETING_INTERVAL > 7; qed");

			let ancient_timestamp = median_timestamp_inclusive(ancient_header.hash(), store);
			let parent_timestamp = median_timestamp_inclusive(parent_header.hash(), store);
			let timestamp_diff = parent_timestamp.checked_sub(ancient_timestamp).unwrap_or_default();
			if timestamp_diff < 43_200 {
				// less than 12h => no difficulty change needed
				return parent_header.bits;
			}

			let mut new_bits: U256 = parent_header.bits.into();
			let max_bits: U256 = max_bits.into();
			new_bits = new_bits + (new_bits >> 2);
			if new_bits > max_bits {
				new_bits = max_bits
			}

			new_bits.into()
		},
		_ => parent_header.bits,
	}
}

pub fn work_required_testnet(parent_hash: H256, time: u32, height: u32, store: &BlockHeaderProvider, network: Magic) -> Compact {
	assert!(height != 0, "cannot calculate required work for genesis block");

	let mut bits = Vec::new();
	let mut block_ref: BlockRef = parent_hash.into();

	let parent_header = store.block_header(block_ref.clone()).expect("height != 0; qed");
	let max_time_gap = parent_header.time + DOUBLE_SPACING_SECONDS;
	if time > max_time_gap {
		return network.max_bits();
	}

	// TODO: optimize it, so it does not make 2016!!! redundant queries each time
	for _ in 0..RETARGETING_INTERVAL {
		let previous_header = match store.block_header(block_ref) {
			Some(h) => h,
			None => { break; }
		};
		bits.push(previous_header.bits);
		block_ref = previous_header.previous_header_hash.into();
	}

	for (index, bit) in bits.into_iter().enumerate() {
		if bit != network.max_bits() || is_retarget_height(height - index as u32 - 1) {
			return bit;
		}
	}

	network.max_bits()
}

/// Algorithm used for retargeting work every 2 weeks
pub fn work_required_retarget(max_work_bits: Compact, retarget_timestamp: u32, last_timestamp: u32, last_bits: Compact) -> Compact {
	let mut retarget: U256 = last_bits.into();
	let maximum: U256 = max_work_bits.into();

	retarget = retarget * retarget_timespan(retarget_timestamp, last_timestamp).into();
	retarget = retarget / TARGET_TIMESPAN_SECONDS.into();

	if retarget > maximum {
		max_work_bits
	} else {
		retarget.into()
	}
}

pub fn block_reward_satoshi(block_height: u32) -> u64 {
	let mut res = 50 * 100 * 1000 * 1000;
	for _ in 0..block_height / 210000 { res /= 2 }
	res
}

#[cfg(test)]
mod tests {
	use std::collections::HashMap;
	use primitives::bytes::Bytes;
	use primitives::hash::H256;
	use primitives::compact::Compact;
	use network::{Magic, ConsensusParams, ConsensusFork};
	use db::{BlockHeaderProvider, BlockRef};
	use chain::BlockHeader;
	use super::{work_required, is_valid_proof_of_work_hash, is_valid_proof_of_work, block_reward_satoshi};

	fn is_valid_pow(max: Compact, bits: u32, hash: &'static str) -> bool {
		is_valid_proof_of_work_hash(bits.into(), &H256::from_reversed_str(hash)) &&
		is_valid_proof_of_work(max.into(), bits.into(), &H256::from_reversed_str(hash))
	}

	#[test]
	fn test_is_valid_proof_of_work() {
		// block 2
		assert!(is_valid_pow(Magic::Mainnet.max_bits(), 486604799u32, "000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"));
		// block 400_000
		assert!(is_valid_pow(Magic::Mainnet.max_bits(), 403093919u32, "000000000000000004ec466ce4732fe6f1ed1cddc2ed4b328fff5224276e3f6f"));

		// other random tests
		assert!(is_valid_pow(Magic::Regtest.max_bits(), 0x181bc330u32, "00000000000000001bc330000000000000000000000000000000000000000000"));
		assert!(!is_valid_pow(Magic::Regtest.max_bits(), 0x181bc330u32, "00000000000000001bc330000000000000000000000000000000000000000001"));
		assert!(!is_valid_pow(Magic::Regtest.max_bits(), 0x181bc330u32, "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"));
	}

	#[test]
	fn reward() {
		assert_eq!(block_reward_satoshi(0), 5000000000);
		assert_eq!(block_reward_satoshi(209999), 5000000000);
		assert_eq!(block_reward_satoshi(210000), 2500000000);
		assert_eq!(block_reward_satoshi(420000), 1250000000);
		assert_eq!(block_reward_satoshi(420001), 1250000000);
		assert_eq!(block_reward_satoshi(629999), 1250000000);
		assert_eq!(block_reward_satoshi(630000), 625000000);
		assert_eq!(block_reward_satoshi(630001), 625000000);
	}

	// original test link:
	// https://github.com/bitcoinclassic/bitcoinclassic/blob/8bf1fb856df44d1b790b0b835e4c1969be736e25/src/test/pow_tests.cpp#L108
	#[test]
	fn bitcoin_cash_req7() {
		#[derive(Default)]
		struct MemoryBlockHeaderProvider {
			pub by_height: Vec<BlockHeader>,
			pub by_hash: HashMap<H256, usize>,
		}

		impl MemoryBlockHeaderProvider {
			pub fn insert(&mut self, header: BlockHeader) {
				self.by_hash.insert(header.hash(), self.by_height.len());
				self.by_height.push(header);
			}
		}

		impl BlockHeaderProvider for MemoryBlockHeaderProvider {
			fn block_header_bytes(&self, _block_ref: BlockRef) -> Option<Bytes> {
				unimplemented!()
			}

			fn block_header(&self, block_ref: BlockRef) -> Option<BlockHeader> {
				match block_ref {
					BlockRef::Hash(ref hash) => self.by_hash.get(hash).map(|h| &self.by_height[*h]).cloned(),
					BlockRef::Number(height) => self.by_height.get(height as usize).cloned(),
				}
			}
		}

		let main_consensus = ConsensusParams::new(Magic::Mainnet, ConsensusFork::NoFork);
		let uahf_consensus = ConsensusParams::new(Magic::Mainnet, ConsensusFork::BitcoinCash(1000));
		let mut header_provider = MemoryBlockHeaderProvider::default();
		header_provider.insert(BlockHeader {
				version: 0,
				previous_header_hash: 0.into(),
				merkle_root_hash: 0.into(),
				time: 1269211443,
				bits: 0x207fffff.into(),
				nonce: 0,
			});

		// create x100 pre-HF blocks
		for height in 1..1000 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 10 * 60;
			header_provider.insert(header);
		}

		// create x10 post-HF blocks every 2 hours
		// MTP still less than 12h
		for height in 1000..1010 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 2 * 60 * 60;
			header_provider.insert(header.clone());

			let main_bits: u32 = work_required(header.hash(), 0, height as u32, &header_provider, &main_consensus).into();
			assert_eq!(main_bits, 0x207fffff_u32);
			let uahf_bits: u32 = work_required(header.hash(), 0, height as u32, &header_provider, &uahf_consensus).into();
			assert_eq!(uahf_bits, 0x207fffff_u32);
		}

		// MTP becames greater than 12h
		let mut header = header_provider.block_header(1009.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 2 * 60 * 60;
		header_provider.insert(header.clone());

		let main_bits: u32 = work_required(header.hash(), 0, 1010, &header_provider, &main_consensus).into();
		assert_eq!(main_bits, 0x207fffff_u32);
		let uahf_bits: u32 = work_required(header.hash(), 0, 1010, &header_provider, &uahf_consensus).into();
		assert_eq!(uahf_bits, 0x1d00ffff_u32);
	}
}
