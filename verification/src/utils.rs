#![allow(dead_code)]
//! Verification utilities
use std::cmp;
use hash::H256;
use uint::U256;
use byteorder::{BigEndian, ByteOrder};
use compact::Compact;

// Timespan constants
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

fn retarget_timespan(retarget_timestamp: u32, last_timestamp: u32) -> u32 {
	let timespan = last_timestamp - retarget_timestamp;
	range_constrain(timespan as u32, MIN_TIMESPAN, MAX_TIMESPAN)
}

pub fn work_required_retarget(max_nbits: u32, retarget_timestamp: u32, last_timestamp: u32, last_nbits: u32) -> u32 {
	// ignore overflows here
	let mut retarget = Compact::new(last_nbits).to_u256().unwrap_or_else(|x| x);
	let maximum = Compact::new(max_nbits).to_u256().unwrap_or_else(|x| x);

	// multiplication overflow potential
	retarget = retarget * U256::from(retarget_timespan(retarget_timestamp, last_timestamp));
	retarget = retarget / U256::from(TARGET_TIMESPAN_SECONDS);

	if retarget > maximum {
		Compact::from_u256(maximum).into()
	} else {
		Compact::from_u256(retarget).into()
	}
}

pub fn work_required_testnet() -> u32 {
	unimplemented!();
}

fn range_constrain(value: u32, min: u32, max: u32) -> u32 {
	cmp::min(cmp::max(value, min), max)
}

/// Simple nbits check that does not require 256-bit arithmetic
pub fn check_nbits(max_nbits: u32, hash: &H256, n_bits: u32) -> bool {
	if n_bits > max_nbits {
		return false;
	}

	let hash_bytes: &[u8] = &**hash;

	let mut nb = [0u8; 4];
	BigEndian::write_u32(&mut nb, n_bits);
	let shift = match nb[0].checked_sub(3) {
		Some(v) => v,
		None => return false,
	} as usize; // total shift for mantissa

	if shift >= 30 { return false; } // invalid shift

	let should_be_zero = shift + 3..32;
	let should_be_le = shift..shift + 3;

	for z_check in should_be_zero {
		if hash_bytes[z_check as usize] != 0 { return false; }
	}

	// making u32 from 3 bytes
	let mut order = 0;
	let hash_val: u32 = hash_bytes[should_be_le].iter()
		.fold(0u32, |s, a| {
			let r = s + ((*a as u32) << order);
			order += 8;
			r
		});

	// using 3 bytes leftover of nbits
	nb[0] = 0;
	let threshold = BigEndian::read_u32(&nb);
	if hash_val < threshold {
		return true;
	}
	else if hash_val > threshold {
		return false;
	}

	// the case when hash effective bits are equal to nbits
	// then the rest of the hash must be zero
	hash_bytes[0..shift].iter().all(|&x| x == 0)
}

pub fn age(protocol_time: u32) -> i64 {
	::time::get_time().sec - protocol_time as i64
}

pub fn block_reward_satoshi(block_height: u32) -> u64 {
	let mut res = 50 * 100 * 1000 * 1000;
	for _ in 0..block_height / 210000 { res /= 2 }
	res
}

#[cfg(test)]
mod tests {
	use network::Magic;
	use super::{block_reward_satoshi, check_nbits};
	use hash::H256;

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

	#[test]
	fn nbits() {
		let max_nbits = Magic::Regtest.max_nbits();

		// strictly equal
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000000");
		let nbits = 0x181bc330u32;
		assert!(check_nbits(max_nbits, &hash, nbits));

		// nbits match but not equal (greater)
		let hash = H256::from_reversed_str("00000000000000001bc330000000000000000000000000000000000000000001");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(max_nbits, &hash, nbits));

		// greater
		let hash = H256::from_reversed_str("ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff");
		let nbits = 0x181bc330u32;
		assert!(!check_nbits(max_nbits, &hash, nbits));


		// some real examples
		let hash = H256::from_reversed_str("000000000000000001f942eb4bfa0aeccb6a14c268f4c72d5fff17270da771b9");
		let nbits = 404129525;
		assert!(check_nbits(max_nbits, &hash, nbits));

		let hash = H256::from_reversed_str("00000000000000000e753ef636075711efd2cbf5a8473c7c5b67755a3701e0c2");
		let nbits = 404129525;
		assert!(check_nbits(max_nbits, &hash, nbits));
	}
}
