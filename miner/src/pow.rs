use std::cmp;
use primitives::compact::Compact;
use primitives::hash::H256;
use primitives::uint::U256;

const RETARGETING_FACTOR: u32 = 4;
const TARGET_TIMESPAN_SECONDS: u32 = 2 * 7 * 24 * 60 * 60;

// The upper and lower bounds for retargeting timespan
const MIN_TIMESPAN: u32 = TARGET_TIMESPAN_SECONDS / RETARGETING_FACTOR;
const MAX_TIMESPAN: u32 = TARGET_TIMESPAN_SECONDS * RETARGETING_FACTOR;

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

#[cfg(test)]
mod tests {
	use super::{is_valid_proof_of_work_hash, is_valid_proof_of_work};
	use hash::H256;

	fn is_valid_pow(max: u32, bits: u32, hash: &'static str) -> bool {
		is_valid_proof_of_work_hash(bits.into(), &H256::from_reversed_str(hash)) &&
		is_valid_proof_of_work(max.into(), bits.into(), &H256::from_reversed_str(hash))
	}

	#[test]
	fn test_is_valid_proof_of_work() {
		// block 2
		assert!(is_valid_pow(0x1d00ffffu32, 486604799u32, "000000006a625f06636b8bb6ac7b960a8d03705d1ace08b1a19da3fdcc99ddbd"));
		// block 400_000
		assert!(is_valid_pow(0x1d00ffffu32, 403093919u32, "000000000000000004ec466ce4732fe6f1ed1cddc2ed4b328fff5224276e3f6f"));
	}
}
