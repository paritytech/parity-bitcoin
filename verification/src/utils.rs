use std::cmp;
use primitives::compact::Compact;
use primitives::hash::H256;
use primitives::uint::U256;
use network::Magic;
use db::{BlockHeaderProvider, BlockRef};

const RETARGETING_FACTOR: u32 = 4;
const TARGET_SPACING_SECONDS: u32 = 10 * 60;
const DOUBLE_SPACING_SECONDS: u32 = 2 * TARGET_SPACING_SECONDS;
const TARGET_TIMESPAN_SECONDS: u32 = 2 * 7 * 24 * 60 * 60;

// The upper and lower bounds for retargeting timespan
const MIN_TIMESPAN: u32 = TARGET_TIMESPAN_SECONDS / RETARGETING_FACTOR;
const MAX_TIMESPAN: u32 = TARGET_TIMESPAN_SECONDS * RETARGETING_FACTOR;

// Target number of blocks, 2 weaks, 2016
pub const RETARGETING_INTERVAL: u32 = TARGET_TIMESPAN_SECONDS / TARGET_SPACING_SECONDS;

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
pub fn work_required(parent_hash: H256, time: u32, height: u32, store: &BlockHeaderProvider, network: Magic) -> Compact {
	assert!(height != 0, "cannot calculate required work for genesis block");

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

		return work_required_retarget(network.max_bits(), retarget_timestamp, last_timestamp, last_bits);
	}

	if network == Magic::Testnet {
		return work_required_testnet(parent_hash, time, height, store, network)
	}

	parent_header.bits
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

pub fn age(protocol_time: u32) -> i64 {
	::time::get_time().sec - protocol_time as i64
}

#[cfg(test)]
mod tests {
	use primitives::hash::H256;
	use primitives::compact::Compact;
	use network::Magic;
	use super::{is_valid_proof_of_work_hash, is_valid_proof_of_work, block_reward_satoshi};

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
}
