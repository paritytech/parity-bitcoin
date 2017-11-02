use std::cmp;
use primitives::compact::Compact;
use primitives::hash::H256;
use primitives::bigint::{Uint, U256};
use chain::BlockHeader;
use network::{Network, ConsensusParams, ConsensusFork};
use db::{BlockHeaderProvider, BlockRef};
use timestamp::median_timestamp_inclusive;

use constants::{
	TARGET_SPACING_SECONDS, DOUBLE_SPACING_SECONDS,
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
	let max_bits = consensus.network.max_bits().into();
	if height == 0 {
		return max_bits;
	}

	let parent_header = store.block_header(parent_hash.clone().into()).expect("self.height != 0; qed");

	// special processing of Bitcoin Cash difficulty adjustment hardfork (Nov 2017), where difficulty is adjusted after each block
	let parent_timestamp = match consensus.fork {
		ConsensusFork::BitcoinCash(ref fork) if height >= fork.height => {
			let parent_timestamp = median_timestamp_inclusive(parent_header.hash(), store);
			if parent_timestamp >= fork.difficulty_adjustion_time {
				return work_required_bitcoin_cash_adjusted(parent_header, time, height, store, consensus);
			}

			Some(parent_timestamp)
		},
		_ => None,
	};

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

	if consensus.network == Network::Testnet {
		return work_required_testnet(parent_hash, time, height, store, Network::Testnet)
	}

	match consensus.fork {
		_ if parent_header.bits == max_bits => parent_header.bits,
		ConsensusFork::BitcoinCash(ref fork) if height >= fork.height => {
			// REQ-7 Difficulty adjustement in case of hashrate drop
			// In case the MTP of the tip of the chain is 12h or more after the MTP 6 block before the tip,
			// the proof of work target is increased by a quarter, or 25%, which corresponds to a difficulty
			// reduction of 20%.
			let ancient_block_ref = (height - 6 - 1).into();
			let ancient_header = store.block_header(ancient_block_ref)
				.expect("parent_header.bits != max_bits; difficulty is max_bits for first RETARGETING_INTERVAL height; RETARGETING_INTERVAL > 7; qed");

			let parent_timestamp = parent_timestamp
				.expect("we are in BitcoinCash-HF branch; parent_timestamp is calculated for BitcoinCash-HF branch above; qed");
			let ancient_timestamp = median_timestamp_inclusive(ancient_header.hash(), store);
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

pub fn work_required_testnet(parent_hash: H256, time: u32, height: u32, store: &BlockHeaderProvider, network: Network) -> Compact {
	assert!(height != 0, "cannot calculate required work for genesis block");

	let mut bits = Vec::new();
	let mut block_ref: BlockRef = parent_hash.into();

	let parent_header = store.block_header(block_ref.clone()).expect("height != 0; qed");
	let max_time_gap = parent_header.time + DOUBLE_SPACING_SECONDS;
	let max_bits = network.max_bits().into();
	if time > max_time_gap {
		return max_bits;
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
		if bit != max_bits || is_retarget_height(height - index as u32 - 1) {
			return bit;
		}
	}

	max_bits
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

/// Algorithm to adjust difficulty after each block. Implementation is based on Bitcoin ABC commit:
/// https://github.com/Bitcoin-ABC/bitcoin-abc/commit/be51cf295c239ff6395a0aa67a3e13906aca9cb2
pub fn work_required_bitcoin_cash_adjusted(parent_header: BlockHeader, time: u32, height: u32, store: &BlockHeaderProvider, consensus: &ConsensusParams) -> Compact {
	/// To reduce the impact of timestamp manipulation, we select the block we are
	/// basing our computation on via a median of 3.
	fn suitable_block(mut header2: BlockHeader, store: &BlockHeaderProvider) -> BlockHeader {
		let reason = "header.height >= RETARGETNG_INTERVAL; RETARGETING_INTERVAL > 2; qed";
		let mut header1 = store.block_header(header2.previous_header_hash.clone().into()).expect(reason);
		let mut header0 = store.block_header(header1.previous_header_hash.clone().into()).expect(reason);

		if header0.time > header2.time {
			::std::mem::swap(&mut header0, &mut header2);
		}
		if header0.time > header1.time {
			::std::mem::swap(&mut header0, &mut header1);
		}
		if header1.time > header2.time {
			::std::mem::swap(&mut header1, &mut header2);
		}

		header1
	}

	/// Get block proof.
	fn block_proof(header: &BlockHeader) -> U256 {
		let proof: U256 = header.bits.into();
		// We need to compute 2**256 / (bnTarget+1), but we can't represent 2**256
		// as it's too large for a arith_uint256. However, as 2**256 is at least as
		// large as bnTarget+1, it is equal to ((2**256 - bnTarget - 1) /
		// (bnTarget+1)) + 1, or ~bnTarget / (nTarget+1) + 1.
		(!proof / (proof + U256::one())) + U256::one()
	}

	/// Compute chain work between two blocks. Last block work is included. First block work is excluded.
	fn compute_work_between_blocks(first: H256, last: &BlockHeader, store: &BlockHeaderProvider) -> U256 {
		debug_assert!(last.hash() != first);
		let mut chain_work: U256 = block_proof(last);
		let mut prev_hash = last.previous_header_hash.clone();
		loop {
			let header = store.block_header(prev_hash.into())
				.expect("last header is on main chain; first is at height last.height - 144; it is on main chain; qed");

			chain_work = chain_work + block_proof(&header);
			prev_hash = header.previous_header_hash;
			if prev_hash == first {
				return chain_work;
			}
		}
	}

	/// Compute the a target based on the work done between 2 blocks and the time
	/// required to produce that work.
	fn compute_target(first_header: BlockHeader, last_header: BlockHeader, store: &BlockHeaderProvider) -> U256 {
		// From the total work done and the time it took to produce that much work,
		// we can deduce how much work we expect to be produced in the targeted time
		// between blocks.
		let mut work = compute_work_between_blocks(first_header.hash(), &last_header, store);
		work = work * TARGET_SPACING_SECONDS.into();

		// In order to avoid difficulty cliffs, we bound the amplitude of the
		// adjustement we are going to do.
		debug_assert!(last_header.time > first_header.time);
		let mut actual_timespan = last_header.time - first_header.time;
		if actual_timespan > 288 * TARGET_SPACING_SECONDS {
			actual_timespan = 288 * TARGET_SPACING_SECONDS;
		} else if actual_timespan < 72 * TARGET_SPACING_SECONDS {
			actual_timespan = 72 * TARGET_SPACING_SECONDS;
		}

		let work = work / actual_timespan.into();

		// We need to compute T = (2^256 / W) - 1 but 2^256 doesn't fit in 256 bits.
		// By expressing 1 as W / W, we get (2^256 - W) / W, and we can compute
		// 2^256 - W as the complement of W.
		(!work) / work
	}
 
	// This cannot handle the genesis block and early blocks in general.
	debug_assert!(height > 0);

	// Special difficulty rule for testnet:
	// If the new block's timestamp is more than 2 * 10 minutes then allow
	// mining of a min-difficulty block.
	let max_bits = consensus.network.max_bits();
	if consensus.network == Network::Testnet {
		let max_time_gap = parent_header.time + DOUBLE_SPACING_SECONDS;
		if time > max_time_gap {
			return max_bits.into();
		}
	}

	// Compute the difficulty based on the full adjustement interval.
	let last_height = height - 1;
	debug_assert!(last_height >= RETARGETING_INTERVAL);

	// Get the last suitable block of the difficulty interval.
	let last_header = suitable_block(parent_header, store);

	// Get the first suitable block of the difficulty interval.
	let first_height = last_height - 144;
	let first_header = store.block_header(first_height.into())
		.expect("last_height >= RETARGETING_INTERVAL; RETARGETING_INTERVAL - 144 > 0; qed");
	let first_header = suitable_block(first_header, store);

	// Compute the target based on time and work done during the interval.
	let next_target = compute_target(first_header, last_header, store);
	let max_bits = consensus.network.max_bits();
	if next_target > max_bits {
		return max_bits.into();
	}

	next_target.into()
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
	use primitives::bigint::U256;
	use primitives::compact::Compact;
	use network::{Network, ConsensusParams, BitcoinCashConsensusParams, ConsensusFork};
	use db::{BlockHeaderProvider, BlockRef};
	use chain::BlockHeader;
	use super::{work_required, is_valid_proof_of_work_hash, is_valid_proof_of_work,
		block_reward_satoshi, work_required_bitcoin_cash_adjusted};

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

	fn is_valid_pow(max: Compact, bits: u32, hash: &'static str) -> bool {
		is_valid_proof_of_work_hash(bits.into(), &H256::from_reversed_str(hash)) &&
		is_valid_proof_of_work(max.into(), bits.into(), &H256::from_reversed_str(hash))
	}

	#[test]
	fn test_is_valid_proof_of_work() {
		// block 2
		assert!(is_valid_pow(Network::Mainnet.max_bits().into(), 486604799u32, "000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"));
		// block 400_000
		assert!(is_valid_pow(Network::Mainnet.max_bits().into(), 403093919u32, "000000000000000004ec466ce4732fe6f1ed1cddc2ed4b328fff5224276e3f6f"));

		// other random tests
		assert!(is_valid_pow(Network::Regtest.max_bits().into(), 0x181bc330u32, "00000000000000001bc330000000000000000000000000000000000000000000"));
		assert!(!is_valid_pow(Network::Regtest.max_bits().into(), 0x181bc330u32, "00000000000000001bc330000000000000000000000000000000000000000001"));
		assert!(!is_valid_pow(Network::Regtest.max_bits().into(), 0x181bc330u32, "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"));
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
		let main_consensus = ConsensusParams::new(Network::Mainnet, ConsensusFork::NoFork);
		let uahf_consensus = ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCash(BitcoinCashConsensusParams {
			height: 1000,
			difficulty_adjustion_time: 0xffffffff,
		}));
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

	// original test link:
	// https://github.com/Bitcoin-ABC/bitcoin-abc/blob/d8eac91f8d16716eed0ad11ccac420122280bb13/src/test/pow_tests.cpp#L193
	#[test]
	fn bitcoin_cash_adjusted_difficulty() {
		let uahf_consensus = ConsensusParams::new(Network::Mainnet, ConsensusFork::BitcoinCash(BitcoinCashConsensusParams {
			height: 1000,
			difficulty_adjustion_time: 0xffffffff,
		}));


		let limit_bits = uahf_consensus.network.max_bits();
		let initial_bits = limit_bits >> 4;
		let mut header_provider = MemoryBlockHeaderProvider::default();

		// Genesis block.
		header_provider.insert(BlockHeader {
				version: 0,
				previous_header_hash: 0.into(),
				merkle_root_hash: 0.into(),
				time: 1269211443,
				bits: initial_bits.into(),
				nonce: 0,
			});

		// Pile up some blocks every 10 mins to establish some history.
		for height in 1..2050 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 600;
			header_provider.insert(header);
		}

		// Difficulty stays the same as long as we produce a block every 10 mins.
		let current_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2049.into()).unwrap(),
			0, 2050, &header_provider, &uahf_consensus);
		for height in 2050..2060 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 600;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);
			debug_assert_eq!(calculated_bits, current_bits);
		}

		// Make sure we skip over blocks that are out of wack. To do so, we produce
		// a block that is far in the future
		let mut header = header_provider.block_header(2059.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 6000;
		header.bits = current_bits;
		header_provider.insert(header);
		let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2060.into()).unwrap(),
			0, 2061, &header_provider, &uahf_consensus);
		debug_assert_eq!(calculated_bits, current_bits);

		// .. and then produce a block with the expected timestamp.
		let mut header = header_provider.block_header(2060.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 2 * 600 - 6000;
		header.bits = current_bits;
		header_provider.insert(header);
		let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2061.into()).unwrap(),
			0, 2062, &header_provider, &uahf_consensus);
		debug_assert_eq!(calculated_bits, current_bits);

		// The system should continue unaffected by the block with a bogous timestamps.
		for height in 2062..2082 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 600;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);
			debug_assert_eq!(calculated_bits, current_bits);
		}

		// We start emitting blocks slightly faster. The first block has no impact.
		let mut header = header_provider.block_header(2081.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 550;
		header.bits = current_bits;
		header_provider.insert(header);
		let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2082.into()).unwrap(),
			0, 2083, &header_provider, &uahf_consensus);
		debug_assert_eq!(calculated_bits, current_bits);

		// Now we should see difficulty increase slowly.
		let mut current_bits = current_bits;
		for height in 2083..2093 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 550;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);

			let current_work: U256 = current_bits.into();
			let calculated_work: U256 = calculated_bits.into();
			debug_assert!(calculated_work < current_work);
			debug_assert!((current_work - calculated_work) < (current_work >> 10));

			current_bits = calculated_bits;
		}

		// Check the actual value.
		debug_assert_eq!(current_bits, 0x1c0fe7b1.into());

		// If we dramatically shorten block production, difficulty increases faster.
		for height in 2093..2113 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 10;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);

			let current_work: U256 = current_bits.into();
			let calculated_work: U256 = calculated_bits.into();
			debug_assert!(calculated_work < current_work);
			debug_assert!((current_work - calculated_work) < (current_work >> 4));

			current_bits = calculated_bits;
		}

		// Check the actual value.
		debug_assert_eq!(current_bits, 0x1c0db19f.into());

		// We start to emit blocks significantly slower. The first block has no
		// impact.
		let mut header = header_provider.block_header(2112.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 6000;
		header.bits = current_bits;
		header_provider.insert(header);
		let mut current_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2113.into()).unwrap(),
			0, 2114, &header_provider, &uahf_consensus);

		// Check the actual value.
		debug_assert_eq!(current_bits, 0x1c0d9222.into());

		// If we dramatically slow down block production, difficulty decreases.
		for height in 2114..2207 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 6000;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);

			let current_work: U256 = current_bits.into();
			let calculated_work: U256 = calculated_bits.into();
			debug_assert!(calculated_work < limit_bits);
			debug_assert!(calculated_work > current_work);
			debug_assert!((calculated_work - current_work) < (current_work >> 3));

			current_bits = calculated_bits;
		}

		// Check the actual value.
		debug_assert_eq!(current_bits, 0x1c2f13b9.into());

		// Due to the window of time being bounded, next block's difficulty actually
		// gets harder.
		let mut header = header_provider.block_header(2206.into()).unwrap();
		header.previous_header_hash = header.hash();
		header.time = header.time + 6000;
		header.bits = current_bits;
		header_provider.insert(header);
		let mut current_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(2207.into()).unwrap(),
			0, 2208, &header_provider, &uahf_consensus);
		debug_assert_eq!(current_bits, 0x1c2ee9bf.into());

		// And goes down again. It takes a while due to the window being bounded and
		// the skewed block causes 2 blocks to get out of the window.
		for height in 2208..2400 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 6000;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);

			let current_work: U256 = current_bits.into();
			let calculated_work: U256 = calculated_bits.into();
			debug_assert!(calculated_work <= limit_bits);
			debug_assert!(calculated_work > current_work);
			debug_assert!((calculated_work - current_work) < (current_work >> 3));

			current_bits = calculated_bits;
		}

		// Check the actual value.
		debug_assert_eq!(current_bits, 0x1d00ffff.into());

		// Once the difficulty reached the minimum allowed level, it doesn't get any
		// easier.
		for height in 2400..2405 {
			let mut header = header_provider.block_header((height - 1).into()).unwrap();
			header.previous_header_hash = header.hash();
			header.time = header.time + 6000;
			header.bits = current_bits;
			header_provider.insert(header);

			let calculated_bits = work_required_bitcoin_cash_adjusted(header_provider.block_header(height.into()).unwrap(),
				0, height + 1, &header_provider, &uahf_consensus);
			debug_assert_eq!(calculated_bits, limit_bits.into());

			current_bits = calculated_bits;
		}
	}
}
